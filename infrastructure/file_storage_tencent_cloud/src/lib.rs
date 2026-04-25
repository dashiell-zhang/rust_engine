use std::time::{Duration, SystemTime, UNIX_EPOCH};

use application_interface::storage::AppFileStorage;
use async_trait::async_trait;
use common::app_error::AppError;
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderMap, HeaderValue};
use reqwest::{Body, Client, Method};
use sha1::{Digest, Sha1};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

/// HMAC-SHA1 签名器类型别名
type HmacSha1 = Hmac<Sha1>;

/// 腾讯云 COS 百分号编码字符集
const COS_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// 腾讯云 COS 文件存储配置
#[derive(Clone)]
pub struct TencentCosFileStorageSettings {
    /// 腾讯云账户 AppId
    pub app_id: String,

    /// COS 存储区域
    pub region: String,

    /// SecretId
    pub secret_id: String,

    /// SecretKey
    pub secret_key: String,

    /// 存储桶名称
    pub bucket_name: String,

    /// 访问地址前缀
    pub url: String,
}

/// 腾讯云 COS 文件存储
pub struct TencentCosFileStorage {
    /// 请求客户端
    client: Client,

    /// 存储配置
    settings: TencentCosFileStorageSettings,

    /// COS API 地址
    endpoint_url: String,

    /// 请求域名
    host: String,
}

impl TencentCosFileStorage {
    /// 创建腾讯云 COS 文件存储
    pub fn new(settings: TencentCosFileStorageSettings) -> Result<Self, AppError> {
        validate_settings(&settings)?;
        let endpoint_url = build_endpoint_url(&settings);
        let host = format!(
            "{}-{}.cos.{}.myqcloud.com",
            settings.bucket_name.trim(),
            settings.app_id.trim(),
            settings.region.trim()
        );

        Ok(Self {
            client: Client::new(),
            settings,
            endpoint_url,
            host,
        })
    }

    /// 规范化对象路径
    fn normalize_object_key(
        &self,
        remote_path: &str,
        file_name: Option<&str>,
    ) -> Result<String, AppError> {
        let normalized = remote_path.trim().replace('\\', "/");

        if normalized
            .split('/')
            .filter(|segment| !segment.is_empty())
            .any(|segment| segment == "." || segment == "..")
        {
            return Err(AppError::bad_request("文件路径非法"));
        }

        let object_key = if let Some(file_name) = file_name {
            let safe_name = std::path::Path::new(file_name.trim())
                .file_name()
                .and_then(|item| item.to_str())
                .filter(|item| !item.is_empty())
                .ok_or_else(|| AppError::bad_request("文件名非法"))?;

            if normalized.is_empty() {
                safe_name.to_string()
            } else {
                format!("{}/{}", normalized.trim_matches('/'), safe_name)
            }
        } else {
            normalized.trim_matches('/').to_string()
        };

        if object_key.is_empty() {
            return Err(AppError::bad_request("文件路径非法"));
        }

        Ok(object_key)
    }

    /// 构建请求地址
    fn object_url(&self, object_key: &str) -> String {
        format!(
            "{}/{}",
            self.endpoint_url.trim_end_matches('/'),
            encode_object_key(object_key)
        )
    }

    /// 构建签名
    fn authorization(&self, method: &Method, object_key: &str) -> Result<String, AppError> {
        let start = current_unix_timestamp()?;
        let end = start + 600;
        let key_time = format!("{start};{end}");
        let sign_key = hmac_sha1_hex(self.settings.secret_key.trim().as_bytes(), &key_time)?;
        let uri_pathname = format!("/{}", encode_object_key(object_key));
        let http_string = format!(
            "{}\n{}\n\nhost={}\n",
            method.as_str().to_ascii_lowercase(),
            uri_pathname,
            self.host
        );
        let hashed_http_string = hex_sha1(http_string.as_bytes());
        let string_to_sign = format!("sha1\n{key_time}\n{hashed_http_string}\n");
        let signature = hmac_sha1_hex(sign_key.as_bytes(), &string_to_sign)?;

        Ok(format!(
            "q-sign-algorithm=sha1&q-ak={}&q-sign-time={}&q-key-time={}&q-header-list=host&q-url-param-list=&q-signature={}",
            self.settings.secret_id.trim(),
            key_time,
            key_time,
            signature
        ))
    }
}

#[async_trait]
impl AppFileStorage for TencentCosFileStorage {
    /// 上传文件
    async fn file_upload(
        &self,
        local_path: &str,
        remote_path: &str,
        is_public_read: bool,
        file_name: Option<&str>,
    ) -> Result<bool, AppError> {
        let object_key = self.normalize_object_key(
            remote_path,
            Some(file_name.unwrap_or_else(|| {
                std::path::Path::new(local_path)
                    .file_name()
                    .and_then(|item| item.to_str())
                    .unwrap_or_default()
            })),
        )?;
        let file = fs::File::open(local_path)
            .await
            .map_err(|error| AppError::internal(format!("打开本地文件失败: {error}")))?;
        let file_length = file
            .metadata()
            .await
            .map_err(|error| AppError::internal(format!("读取本地文件信息失败: {error}")))?
            .len();
        let stream = ReaderStream::new(file);
        let authorization = self.authorization(&Method::PUT, &object_key)?;

        let mut headers = HeaderMap::new();
        headers.insert(HOST, header_value(&self.host)?);
        headers.insert(AUTHORIZATION, header_value(&authorization)?);
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        headers.insert(CONTENT_LENGTH, header_value(&file_length.to_string())?);
        headers.insert(
            "x-cos-acl",
            HeaderValue::from_static(if is_public_read {
                "public-read"
            } else {
                "private"
            }),
        );

        let response = self
            .client
            .put(self.object_url(&object_key))
            .headers(headers)
            .body(Body::wrap_stream(stream))
            .send()
            .await
            .map_err(|error| AppError::internal(format!("腾讯云 COS 上传失败: {error}")))?;

        Ok(response.status().is_success())
    }

    /// 下载文件
    async fn file_download(&self, remote_path: &str, local_path: &str) -> Result<bool, AppError> {
        let object_key = self.normalize_object_key(remote_path, None)?;
        let authorization = self.authorization(&Method::GET, &object_key)?;
        let mut response = self
            .client
            .get(self.object_url(&object_key))
            .header(HOST, &self.host)
            .header(AUTHORIZATION, authorization)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("腾讯云 COS 下载失败: {error}")))?;

        if !response.status().is_success() {
            return Ok(false);
        }

        if let Some(parent) = std::path::Path::new(local_path).parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| AppError::internal(format!("创建文件目录失败: {error}")))?;
        }

        let mut file = fs::File::create(local_path)
            .await
            .map_err(|error| AppError::internal(format!("创建本地文件失败: {error}")))?;

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| AppError::internal(format!("腾讯云 COS 读取响应失败: {error}")))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|error| AppError::internal(format!("写入本地文件失败: {error}")))?;
        }

        file.flush()
            .await
            .map_err(|error| AppError::internal(format!("刷新本地文件失败: {error}")))?;

        Ok(true)
    }

    /// 删除文件
    async fn file_delete(&self, remote_path: &str) -> Result<bool, AppError> {
        let object_key = self.normalize_object_key(remote_path, None)?;
        let authorization = self.authorization(&Method::DELETE, &object_key)?;
        let response = self
            .client
            .delete(self.object_url(&object_key))
            .header(HOST, &self.host)
            .header(AUTHORIZATION, authorization)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("腾讯云 COS 删除失败: {error}")))?;

        Ok(response.status().is_success())
    }

    /// 获取文件临时访问地址
    fn get_file_url(&self, remote_path: &str, expiry: Duration, is_inline: bool) -> Option<String> {
        let object_key = self.normalize_object_key(remote_path, None).ok()?;
        let start = current_unix_timestamp().ok()?;
        let end = start + i64::try_from(expiry.as_secs()).ok()?;
        let key_time = format!("{start};{end}");
        let sign_key = hmac_sha1_hex(self.settings.secret_key.trim().as_bytes(), &key_time).ok()?;
        let mut params = Vec::new();

        if is_inline {
            params.push(("response-content-disposition", "inline".to_string()));
        }

        let http_parameters = params
            .iter()
            .map(|(key, value)| {
                format!("{}={}", cos_percent_encode(key), cos_percent_encode(value))
            })
            .collect::<Vec<_>>()
            .join("&");
        let uri_pathname = format!("/{}", encode_object_key(&object_key));
        let http_string = format!(
            "get\n{}\n{}\nhost={}\n",
            uri_pathname, http_parameters, self.host
        );
        let hashed_http_string = hex_sha1(http_string.as_bytes());
        let string_to_sign = format!("sha1\n{key_time}\n{hashed_http_string}\n");
        let signature = hmac_sha1_hex(sign_key.as_bytes(), &string_to_sign).ok()?;

        let mut url = format!(
            "{}{}/{}?q-sign-algorithm=sha1&q-ak={}&q-sign-time={}&q-key-time={}&q-header-list=host&q-url-param-list={}&q-signature={}",
            self.settings.url.trim_end_matches('/'),
            if self.settings.url.ends_with('/') {
                ""
            } else {
                "/"
            },
            encode_object_key(&object_key),
            self.settings.secret_id.trim(),
            key_time,
            key_time,
            params
                .iter()
                .map(|(key, _)| cos_percent_encode(key))
                .collect::<Vec<_>>()
                .join(";"),
            signature
        );

        for (key, value) in params {
            url.push('&');
            url.push_str(&cos_percent_encode(key));
            url.push('=');
            url.push_str(&cos_percent_encode(&value));
        }

        Some(url)
    }
}

/// 校验 COS 配置
fn validate_settings(settings: &TencentCosFileStorageSettings) -> Result<(), AppError> {
    if settings.bucket_name.trim().is_empty() {
        return Err(AppError::internal(
            "腾讯云 COS bucket_name 未配置，请检查 [file.tencent_cos].bucket_name",
        ));
    }

    if settings.secret_id.trim().is_empty() {
        return Err(AppError::internal(
            "腾讯云 COS secret_id 未配置，请检查 [file.tencent_cos].secret_id",
        ));
    }

    if settings.secret_key.trim().is_empty() {
        return Err(AppError::internal(
            "腾讯云 COS secret_key 未配置，请检查 [file.tencent_cos].secret_key",
        ));
    }

    if settings.app_id.trim().is_empty() {
        return Err(AppError::internal(
            "腾讯云 COS app_id 未配置，请检查 [file.tencent_cos].app_id",
        ));
    }

    if settings.region.trim().is_empty() {
        return Err(AppError::internal(
            "腾讯云 COS region 未配置，请检查 [file.tencent_cos].region",
        ));
    }

    if settings.url.trim().is_empty() {
        return Err(AppError::internal(
            "文件访问地址未配置，请检查 [file].public_base_url",
        ));
    }

    Ok(())
}

/// 构建 COS API 地址
fn build_endpoint_url(settings: &TencentCosFileStorageSettings) -> String {
    format!(
        "https://{}-{}.cos.{}.myqcloud.com",
        settings.bucket_name.trim(),
        settings.app_id.trim(),
        settings.region.trim()
    )
}

/// 获取当前 Unix 时间戳
fn current_unix_timestamp() -> Result<i64, AppError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| AppError::internal(format!("获取系统时间失败: {error}")))?;

    Ok(duration.as_secs() as i64)
}

/// 生成 HMAC-SHA1 十六进制摘要
fn hmac_sha1_hex(key: &[u8], payload: &str) -> Result<String, AppError> {
    let mut mac = HmacSha1::new_from_slice(key)
        .map_err(|error| AppError::internal(format!("腾讯云 COS 签名初始化失败: {error}")))?;
    mac.update(payload.as_bytes());
    Ok(hex_lower(&mac.finalize().into_bytes()))
}

/// 生成 SHA1 十六进制摘要
fn hex_sha1(payload: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(payload);
    hex_lower(&hasher.finalize())
}

/// 十六进制小写编码
fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 构建 HeaderValue
fn header_value(value: &str) -> Result<HeaderValue, AppError> {
    HeaderValue::from_str(value)
        .map_err(|error| AppError::internal(format!("构建请求头失败: {error}")))
}

/// 编码对象路径
fn encode_object_key(object_key: &str) -> String {
    object_key
        .split('/')
        .map(cos_percent_encode)
        .collect::<Vec<_>>()
        .join("/")
}

/// 腾讯云 COS 百分号编码
fn cos_percent_encode(value: &str) -> String {
    utf8_percent_encode(value, COS_ENCODE_SET).to_string()
}
