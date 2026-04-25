use std::fmt::Write;
use std::time::{Duration, SystemTime};

use application_interface::storage::AppFileStorage;
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use common::app_error::AppError;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, DATE, HeaderMap, HeaderValue};
use reqwest::{Body, Client, Method, StatusCode};
use sha1::Sha1;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

/// HMAC-SHA1 签名器类型别名
type HmacSha1 = Hmac<Sha1>;

/// 阿里云 OSS 文件存储配置
#[derive(Clone)]
pub struct AliyunOssFileStorageSettings {
    /// OSS 存储区域
    pub region: String,

    /// 是否使用内网节点
    pub use_internal_endpoint: bool,

    /// AccessKeyId
    pub access_key_id: String,

    /// AccessKeySecret
    pub access_key_secret: String,

    /// 存储桶名称
    pub bucket_name: String,

    /// 访问地址前缀
    pub url: String,
}

/// 阿里云 OSS 文件存储
pub struct AliyunOssFileStorage {
    /// 请求客户端
    client: Client,

    /// 存储配置
    settings: AliyunOssFileStorageSettings,

    /// OSS 直连基地址
    endpoint_url: String,
}

impl AliyunOssFileStorage {
    /// 创建阿里云 OSS 文件存储
    pub fn new(settings: AliyunOssFileStorageSettings) -> Result<Self, AppError> {
        validate_settings(&settings)?;

        Ok(Self {
            client: Client::new(),
            endpoint_url: build_endpoint_url(&settings),
            settings,
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

    /// 构建对象访问地址
    fn object_url(&self, object_key: &str) -> String {
        format!(
            "{}/{}",
            self.endpoint_url.trim_end_matches('/'),
            encode_object_key(object_key)
        )
    }

    /// 构建授权请求
    fn build_request(
        &self,
        method: Method,
        object_key: &str,
        content_length: Option<u64>,
    ) -> Result<reqwest::RequestBuilder, AppError> {
        let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let canonical_resource = format!("/{}/{}", self.settings.bucket_name, object_key);
        let content_type = if content_length.is_some() {
            "application/octet-stream"
        } else {
            ""
        };
        let string_to_sign = format!(
            "{}\n\n{}\n{}\n{}",
            method.as_str(),
            content_type,
            date,
            canonical_resource
        );
        let signature = sign(&self.settings.access_key_secret, &string_to_sign)?;
        let authorization = format!("OSS {}:{}", self.settings.access_key_id, signature);

        let mut headers = HeaderMap::new();
        headers.insert(DATE, header_value(&date)?);
        headers.insert(AUTHORIZATION, header_value(&authorization)?);

        if let Some(content_length) = content_length {
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            headers.insert(
                CONTENT_LENGTH,
                header_value(content_length.to_string().as_str())?,
            );
        }

        Ok(self
            .client
            .request(method, self.object_url(object_key))
            .headers(headers))
    }
}

#[async_trait]
impl AppFileStorage for AliyunOssFileStorage {
    /// 上传文件
    async fn file_upload(
        &self,
        local_path: &str,
        remote_path: &str,
        _is_public_read: bool,
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
        let response = self
            .build_request(Method::PUT, &object_key, Some(file_length))?
            .body(Body::wrap_stream(stream))
            .send()
            .await
            .map_err(|error| AppError::internal(format!("阿里云 OSS 上传失败: {error}")))?;

        Ok(response.status().is_success())
    }

    /// 下载文件
    async fn file_download(&self, remote_path: &str, local_path: &str) -> Result<bool, AppError> {
        let object_key = self.normalize_object_key(remote_path, None)?;
        let mut response = self
            .build_request(Method::GET, &object_key, None)?
            .send()
            .await
            .map_err(|error| AppError::internal(format!("阿里云 OSS 下载失败: {error}")))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }

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
            .map_err(|error| AppError::internal(format!("阿里云 OSS 读取响应失败: {error}")))?
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
        let response = self
            .build_request(Method::DELETE, &object_key, None)?
            .send()
            .await
            .map_err(|error| AppError::internal(format!("阿里云 OSS 删除失败: {error}")))?;

        Ok(response.status() == StatusCode::NO_CONTENT || response.status() == StatusCode::OK)
    }

    /// 获取文件临时访问地址
    fn get_file_url(&self, remote_path: &str, expiry: Duration, is_inline: bool) -> Option<String> {
        let object_key = self.normalize_object_key(remote_path, None).ok()?;
        let expires_at = SystemTime::now().checked_add(expiry)?;
        let expires_unix = expires_at
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs();
        let canonical_resource = format!("/{}/{}", self.settings.bucket_name, object_key);
        let response_disposition = if is_inline {
            Some("inline".to_string())
        } else {
            None
        };

        let string_to_sign = format!(
            "GET\n\n\n{}\n{}{}",
            expires_unix,
            canonical_resource,
            response_disposition
                .as_ref()
                .map_or(String::new(), |value| {
                    format!("?response-content-disposition={value}")
                })
        );
        let signature = sign(&self.settings.access_key_secret, &string_to_sign).ok()?;
        let mut url = format!(
            "{}{}/{}?OSSAccessKeyId={}&Expires={}&Signature={}",
            self.settings.url.trim_end_matches('/'),
            if self.settings.url.ends_with('/') {
                ""
            } else {
                "/"
            },
            encode_object_key(&object_key),
            self.settings.access_key_id,
            expires_unix,
            urlencoding::encode(&signature)
        );

        if let Some(value) = response_disposition {
            let _ = write!(
                url,
                "&response-content-disposition={}",
                urlencoding::encode(&value)
            );
        }

        Some(url)
    }
}

/// 校验 OSS 配置
fn validate_settings(settings: &AliyunOssFileStorageSettings) -> Result<(), AppError> {
    if settings.bucket_name.trim().is_empty() {
        return Err(AppError::internal(
            "阿里云 OSS bucket_name 未配置，请检查 [file.aliyun_oss].bucket_name",
        ));
    }

    if settings.access_key_id.trim().is_empty() {
        return Err(AppError::internal(
            "阿里云 OSS access_key_id 未配置，请检查 [file.aliyun_oss].access_key_id",
        ));
    }

    if settings.access_key_secret.trim().is_empty() {
        return Err(AppError::internal(
            "阿里云 OSS access_key_secret 未配置，请检查 [file.aliyun_oss].access_key_secret",
        ));
    }

    if settings.region.trim().is_empty() {
        return Err(AppError::internal(
            "阿里云 OSS region 未配置，请检查 [file.aliyun_oss].region",
        ));
    }

    if settings.url.trim().is_empty() {
        return Err(AppError::internal(
            "文件访问地址未配置，请检查 [file].public_base_url",
        ));
    }

    Ok(())
}

/// 构建 OSS 直连地址
fn build_endpoint_url(settings: &AliyunOssFileStorageSettings) -> String {
    let internal_segment = if settings.use_internal_endpoint {
        "-internal"
    } else {
        ""
    };

    format!(
        "https://{}.oss-{}{}.aliyuncs.com",
        settings.bucket_name.trim(),
        settings.region.trim(),
        internal_segment
    )
}

/// 生成签名
fn sign(secret: &str, string_to_sign: &str) -> Result<String, AppError> {
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes())
        .map_err(|error| AppError::internal(format!("阿里云 OSS 签名初始化失败: {error}")))?;
    mac.update(string_to_sign.as_bytes());
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

/// 构建 HeaderValue
fn header_value(value: &str) -> Result<HeaderValue, AppError> {
    HeaderValue::from_str(value)
        .map_err(|error| AppError::internal(format!("构建请求头失败: {error}")))
}

/// 编码对象路径
fn encode_object_key(object_key: &str) -> String {
    let mut output = String::with_capacity(object_key.len());

    for (index, segment) in object_key.split('/').enumerate() {
        if index > 0 {
            output.push('/');
        }

        for byte in segment.as_bytes() {
            let ch = *byte as char;
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
                output.push(ch);
            } else {
                let _ = write!(output, "%{byte:02X}");
            }
        }
    }

    output
}
