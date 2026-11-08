use std::{
    collections::BTreeMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use application_interface::{http_client::HttpClientFactory, storage::AppFileStorage};
use async_trait::async_trait;
use chrono::TimeDelta;
use common::app_error::AppError;
use common::date_time_helper::time_span_ticks;
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::Method;
use sha1::{Digest, Sha1};
use std::sync::Arc;
use tokio::fs;

mod download;
mod request;
mod upload;
mod upload_xml;

/// HMAC-SHA1 签名器类型别名
type HmacSha1 = Hmac<Sha1>;

struct SigningCredentials {
    key_time: String,
    sign_key: String,
}

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
    http_clients: Arc<HttpClientFactory>,

    /// 存储配置
    settings: TencentCosFileStorageSettings,

    /// COS API 地址
    endpoint_url: String,

    /// 请求域名
    host: String,
}

impl TencentCosFileStorage {
    /// 创建腾讯云 COS 文件存储
    pub fn new(
        mut settings: TencentCosFileStorageSettings,
        http_clients: Arc<HttpClientFactory>,
    ) -> Result<Self, AppError> {
        validate_settings(&settings)?;
        // DefaultQCloudCredentialProvider 先拒绝空串，再裁剪两端空白。
        settings.secret_id = settings.secret_id.trim().to_owned();
        settings.secret_key = settings.secret_key.trim().to_owned();
        let endpoint_url = build_endpoint_url(&settings);
        let host = endpoint_url
            .strip_prefix("https://")
            .expect("固定 HTTPS 前缀")
            .to_owned();

        Ok(Self {
            http_clients,
            settings,
            endpoint_url,
            host,
        })
    }

    /// 构建请求地址
    fn object_url(&self, object_key: &str) -> String {
        format!(
            "{}{}",
            self.endpoint_url.trim_end_matches('/'),
            encode_object_key(&object_path(object_key))
        )
    }

    /// 默认永久凭据每次派生新的 600 秒密钥区间，不跨请求缓存。
    fn signing_credentials(&self) -> Result<SigningCredentials, AppError> {
        let start = current_unix_timestamp()?;
        let end = start
            .checked_add(600)
            .ok_or_else(|| AppError::internal("腾讯云 COS 密钥过期时间超出范围"))?;
        let key_time = format!("{start};{end}");
        let sign_key = hmac_sha1_hex(self.settings.secret_key.as_bytes(), &key_time)?;
        Ok(SigningCredentials { key_time, sign_key })
    }

    fn authorization_with_headers(
        &self,
        method: &Method,
        object_key: &str,
        additional: &[(&str, &str)],
    ) -> Result<String, AppError> {
        self.request_authorization(method, object_key, additional, &[])
    }

    fn request_authorization(
        &self,
        method: &Method,
        object_key: &str,
        additional: &[(&str, &str)],
        query: &[(&str, &str)],
    ) -> Result<String, AppError> {
        let SigningCredentials { key_time, sign_key } = self.signing_credentials()?;
        let uri_pathname = object_path(object_key);
        let mut headers = BTreeMap::from([("host", self.host.as_str())]);
        headers.extend(additional.iter().copied());
        let header_names = headers.keys().copied().collect::<Vec<_>>().join(";");
        let header_values = headers
            .iter()
            .map(|(name, value)| {
                format!("{}={}", cos_percent_encode(name), cos_percent_encode(value))
            })
            .collect::<Vec<_>>()
            .join("&");
        // COS 签名使用小写、排序后的查询名；实际 URL 保留 API 参数原名。
        let query: BTreeMap<_, _> = query
            .iter()
            .map(|(name, value)| (name.to_ascii_lowercase(), *value))
            .collect();
        let query_names = query
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(";");
        let query_values = query
            .iter()
            .map(|(name, value)| {
                format!("{}={}", cos_percent_encode(name), cos_percent_encode(value))
            })
            .collect::<Vec<_>>()
            .join("&");
        let http_string = format!(
            "{}\n{}\n{}\n{}\n",
            method.as_str().to_ascii_lowercase(),
            uri_pathname,
            query_values,
            header_values
        );
        let hashed_http_string = hex_sha1(http_string.as_bytes());
        let string_to_sign = format!("sha1\n{key_time}\n{hashed_http_string}\n");
        let signature = hmac_sha1_hex(sign_key.as_bytes(), &string_to_sign)?;

        Ok(format!(
            "q-sign-algorithm=sha1&q-ak={}&q-sign-time={}&q-key-time={}&q-header-list={header_names}&q-url-param-list={query_names}&q-signature={}",
            self.settings.secret_id, key_time, key_time, signature
        ))
    }
}

#[async_trait]
impl AppFileStorage for TencentCosFileStorage {
    /// 与 OSS 一致：目录加本地文件名作为对象键，展示名称只用于下载响应头。
    async fn file_upload(
        &self,
        local_path: &str,
        remote_path: &str,
        is_public_read: bool,
        file_name: Option<&str>,
    ) -> Result<bool, AppError> {
        let basename = Path::new(local_path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AppError::internal("本地文件名无效"))?;
        let object_key = Path::new(remote_path)
            .join(basename)
            .to_string_lossy()
            .replace('\\', "/");
        self.upload_object(local_path, &object_key, is_public_read, file_name)
            .await
    }

    /// 下载文件
    async fn file_download(&self, remote_path: &str, local_path: &str) -> Result<bool, AppError> {
        // 源组件先按最后一个正斜杠取得目录并创建，再发起远程下载。
        let local_directory = local_path
            .rfind('/')
            .map(|index| &local_path[..=index])
            .filter(|directory| !directory.is_empty())
            .ok_or_else(|| AppError::internal("腾讯云 COS 下载目标缺少目录"))?;
        fs::create_dir_all(local_directory)
            .await
            .map_err(|error| AppError::internal(format!("创建文件目录失败: {error}")))?;
        self.download_object(remote_path, local_path, local_directory)
            .await
    }

    /// 删除文件
    async fn file_delete(&self, remote_path: &str) -> Result<bool, AppError> {
        let response = self
            .execute_request(
                Method::DELETE,
                remote_path,
                &[],
                BTreeMap::new(),
                request::RequestBody::Empty,
            )
            .await?;

        Ok(response.status.is_success())
    }

    /// 获取文件临时访问地址
    fn get_file_url(
        &self,
        remote_path: &str,
        expiry: TimeDelta,
        is_inline: bool,
    ) -> Result<Option<String>, AppError> {
        let object_key = remote_path.replace('\\', "/");
        let ticks = time_span_ticks(expiry)
            .ok_or_else(|| AppError::internal("腾讯云 COS 签名有效期超出范围"))?;
        // TimeSpan.TotalSeconds 先转换 double，Convert.ToInt64 再按中点到偶数取整。
        let seconds = (ticks as f64 / 10_000_000.0).round_ties_even() as i64;
        let start = current_unix_timestamp()?;
        let requested_sign_time = if seconds > 0 {
            let end = start
                .checked_add(seconds)
                .ok_or_else(|| AppError::internal("腾讯云 COS 签名过期时间超出范围"))?;
            Some(format!("{start};{end}"))
        } else {
            None
        };
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
        let SigningCredentials { key_time, sign_key } = self.signing_credentials()?;
        let sign_time = requested_sign_time.as_deref().unwrap_or(&key_time);
        let uri_pathname = object_path(&object_key);
        // 源 PreSignatureStruct 默认不签入 Host，可替换为配置中的访问域名。
        let http_string = format!("get\n{uri_pathname}\n{http_parameters}\n\n");
        let hashed_http_string = hex_sha1(http_string.as_bytes());
        let string_to_sign = format!("sha1\n{sign_time}\n{hashed_http_string}\n");
        let signature = hmac_sha1_hex(sign_key.as_bytes(), &string_to_sign)?;

        let query_prefix = if http_parameters.is_empty() {
            String::new()
        } else {
            format!("{http_parameters}&")
        };
        let url = format!(
            "{}{}?{query_prefix}q-sign-algorithm=sha1&q-ak={}&q-sign-time={}&q-key-time={}&q-header-list=&q-url-param-list={}&q-signature={}",
            self.endpoint_url,
            encode_object_key(&uri_pathname),
            cos_percent_encode(&self.settings.secret_id),
            sign_time,
            key_time,
            params
                .iter()
                .map(|(key, _)| cos_percent_encode(key))
                .collect::<Vec<_>>()
                .join(";"),
            signature
        );

        let url =
            reqwest::Url::parse(&url).map_err(|_| AppError::internal("腾讯云 COS 临时地址无效"))?;
        Ok(Some(format!(
            "{}{}?{}",
            self.settings.url,
            url.path()
                .strip_prefix('/')
                .ok_or_else(|| AppError::internal("腾讯云 COS 临时地址路径无效"))?,
            url.query()
                .ok_or_else(|| AppError::internal("腾讯云 COS 临时地址缺少签名"))?
        )))
    }
}

/// 校验 COS 配置
fn validate_settings(settings: &TencentCosFileStorageSettings) -> Result<(), AppError> {
    // CosXmlConfig.Builder.SetRegion 在创建凭据提供商之前执行。
    if settings.region.is_empty() || settings.region.contains('@') {
        return Err(AppError::internal("腾讯云 COS region 为空或包含无效字符"));
    }

    if settings.secret_id.is_empty() {
        return Err(AppError::internal(
            "腾讯云 COS secret_id 未配置，请检查 [file.tencent_cos].secret_id",
        ));
    }

    if settings.secret_key.is_empty() {
        return Err(AppError::internal(
            "腾讯云 COS secret_key 未配置，请检查 [file.tencent_cos].secret_key",
        ));
    }

    Ok(())
}

/// 构建 COS API 地址
fn build_endpoint_url(settings: &TencentCosFileStorageSettings) -> String {
    let suffix = format!("-{}", settings.app_id);
    let bucket = if settings.bucket_name.ends_with(&suffix) {
        settings.bucket_name.clone()
    } else {
        format!("{}{suffix}", settings.bucket_name)
    };
    format!("https://{bucket}.cos.{}.myqcloud.com", settings.region)
}

/// 只补齐缺少的前导斜杠，签名使用未编码的原始路径。
fn object_path(key: &str) -> String {
    if key.starts_with('/') {
        key.to_owned()
    } else {
        format!("/{key}")
    }
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
