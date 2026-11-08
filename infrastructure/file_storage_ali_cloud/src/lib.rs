use std::{
    collections::BTreeMap,
    fmt::Write,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, TimeDelta, Timelike, Utc};
use common::app_error::AppError;
use common::date_time_helper::{time_span_from_ticks, time_span_ticks};
use file_storage::AppFileStorage;
use hmac::{Hmac, KeyInit, Mac};
use http_client::HttpClientFactory;
use reqwest::{Method, StatusCode};
use sha2::{Digest, Sha256};

mod content_type;
mod download;
mod integrity;
mod request;
mod service_error;
mod upload;

use upload::UploadSource;

type HmacSha256 = Hmac<Sha256>;
const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";
const READ_WRITE_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_ATTEMPTS: usize = 3;

/// 阿里云 OSS 文件存储配置。
#[derive(Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct AliyunOssFileStorageSettings {
    /// OSS 存储区域。
    pub region: String,
    /// 是否使用内网节点。
    pub use_internal_endpoint: bool,
    /// 访问凭据标识。
    pub access_key_id: String,
    /// 访问凭据密钥。
    pub access_key_secret: String,
    /// 存储桶名称。
    pub bucket_name: String,
    /// 对外访问前缀，按配置原文拼接对象路径及查询串。
    #[serde(skip_deserializing)]
    pub url: String,
    /// 兼容旧 TOML 字段，原装配没有使用它，保持按区域构造 API 地址。
    pub endpoint: String,
}

/// OSS V4 存储适配器，连接池由宿主共享。
pub struct AliyunOssFileStorage {
    http_clients: Arc<HttpClientFactory>,
    settings: AliyunOssFileStorageSettings,
    endpoint_url: String,
    clock_offset: AtomicI64,
}

impl AliyunOssFileStorage {
    /// 使用宿主工厂装配存储，不创建独立的长期 HTTP 客户端。
    pub fn new(
        settings: AliyunOssFileStorageSettings,
        http_clients: Arc<HttpClientFactory>,
    ) -> Result<Self, AppError> {
        // SDK 在真正调用时校验桶、对象、端点和凭据，构造组件不提前失败。
        let internal = if settings.use_internal_endpoint {
            "-internal"
        } else {
            ""
        };
        let endpoint_url = format!(
            "https://{}.oss-{}{internal}.aliyuncs.com",
            settings.bucket_name, settings.region
        );
        Ok(Self {
            settings,
            http_clients,
            endpoint_url,
            clock_offset: AtomicI64::new(0),
        })
    }

    fn validate_operation(&self, object_key: &str) -> Result<(), AppError> {
        let bucket = &self.settings.bucket_name;
        if !(3..=64).contains(&bucket.len())
            || bucket.starts_with('-')
            || bucket.ends_with('-')
            || !valid_endpoint_name(bucket)
        {
            return Err(AppError::internal("阿里云 OSS 存储桶名称无效"));
        }
        // C# String.Length 按 UTF-16 码元计数，不按 UTF-8 字节数限制中文对象名。
        if !(1..=1024).contains(&object_key.encode_utf16().count()) {
            return Err(AppError::internal("阿里云 OSS 对象名称长度无效"));
        }
        if !valid_endpoint_name(&self.settings.region) {
            return Err(AppError::internal("阿里云 OSS 区域端点无效"));
        }
        if self.settings.access_key_id.is_empty() || self.settings.access_key_secret.is_empty() {
            return Err(AppError::internal("阿里云 OSS 访问凭据为空"));
        }
        Ok(())
    }

    /// 资源签名保留桶名和原始对象键，只有 URI 编码改变字节表示。
    fn canonical_uri(&self, object_key: &str) -> String {
        encode_path(&format!("/{}/{}", self.settings.bucket_name, object_key))
    }

    /// 与 OSS V4 相同的日期、区域及产品密钥派生。
    fn signature(
        &self,
        time: DateTime<Utc>,
        canonical_request: &str,
    ) -> Result<(String, String), AppError> {
        let date = time.format("%Y%m%d").to_string();
        let scope = format!("{date}/{}/oss/aliyun_v4_request", self.settings.region);
        let string_to_sign = format!(
            "OSS4-HMAC-SHA256\n{}\n{scope}\n{}",
            time.format("%Y%m%dT%H%M%SZ"),
            hex(&Sha256::digest(canonical_request.as_bytes()))
        );
        let mut key = format!("aliyun_v4{}", self.settings.access_key_secret).into_bytes();
        for part in [
            &date,
            &self.settings.region,
            "oss",
            "aliyun_v4_request",
            &string_to_sign,
        ] {
            key = hmac_sha256(&key, part)?;
        }
        Ok((scope, hex(&key)))
    }

    /// 签入 content-type 和全部 x-oss 头，ACL 随上传请求一起认证。
    fn build_request(
        &self,
        method: Method,
        object_key: &str,
        mut headers: BTreeMap<&str, String>,
        clock_offset: i64,
    ) -> Result<reqwest::RequestBuilder, AppError> {
        self.validate_operation(object_key)?;
        let now = signing_time(clock_offset)?;
        headers.insert("x-oss-content-sha256", UNSIGNED_PAYLOAD.to_owned());
        headers.insert("x-oss-date", now.format("%Y%m%dT%H%M%SZ").to_string());
        let canonical_headers = headers
            .iter()
            .filter(|(key, _)| {
                key.starts_with("x-oss-") || **key == "content-type" || **key == "content-md5"
            })
            .map(|(key, value)| format!("{key}:{}\n", value.trim()))
            .collect::<String>();
        let canonical = format!(
            "{}\n{}\n\n{canonical_headers}\n\n{UNSIGNED_PAYLOAD}",
            method.as_str(),
            self.canonical_uri(object_key)
        );
        let (scope, signature) = self.signature(now, &canonical)?;
        // 请求层分别控制 SDK 读写期限和宿主 HttpClient 的整体期限。
        let mut request = self
            .http_clients
            .streaming_client()
            .request(
                method,
                format!("{}/{}", self.endpoint_url, encode_path(object_key)),
            )
            .header("date", now.format("%a, %d %b %Y %H:%M:%S GMT").to_string())
            .header(
                "authorization",
                format!(
                    "OSS4-HMAC-SHA256 Credential={}/{scope},Signature={signature}",
                    self.settings.access_key_id
                ),
            );
        for (name, value) in headers {
            request = request.header(name, value);
        }
        Ok(request)
    }
}

#[async_trait]
impl AppFileStorage for AliyunOssFileStorage {
    /// 远程目录与本地文件名构成对象键；file_name 仅用于下载响应头。
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
        let source = UploadSource::open(local_path).await?;
        let mut headers = BTreeMap::from([
            (
                "content-type",
                content_type::for_object(&object_key).to_owned(),
            ),
            ("content-length", source.length.to_string()),
            (
                "x-oss-object-acl",
                if is_public_read {
                    "public-read"
                } else {
                    "private"
                }
                .to_owned(),
            ),
        ]);
        if let Some(name) = file_name {
            headers.insert(
                "content-disposition",
                format!("attachment;filename*=UTF-8''{}", encode(name)),
            );
        }
        let response = self
            .execute_request(Method::PUT, &object_key, headers, Some(&source))
            .await?;
        Ok(response.status == StatusCode::OK)
    }

    /// 下载至指定文件；父目录由调用方负责提供。
    async fn file_download(&self, remote_path: &str, local_path: &str) -> Result<bool, AppError> {
        for attempt in 1..=DOWNLOAD_ATTEMPTS {
            let response = self
                .execute_request(Method::GET, remote_path, BTreeMap::new(), None)
                .await?;
            match download::write_response(response, local_path).await {
                Ok(result) => return Ok(result),
                Err(error) if attempt == DOWNLOAD_ATTEMPTS => return Err(error),
                // SDK 的文件拷贝层立即重新下载，不拼接残片或沿用失败的摘要。
                Err(_) => {}
            }
        }
        unreachable!("下载次数固定大于零，末次失败直接返回")
    }

    /// 保留源组件对删除响应 200 的判断，不把 204 改为 true。
    async fn file_delete(&self, remote_path: &str) -> Result<bool, AppError> {
        let response = self
            .execute_request(Method::DELETE, remote_path, BTreeMap::new(), None)
            .await?;
        Ok(response.status == StatusCode::OK)
    }

    /// 生成 OSS V4 临时地址，再按源组件替换为配置中的访问前缀。
    fn get_file_url(
        &self,
        remote_path: &str,
        expiry: TimeDelta,
        is_inline: bool,
    ) -> Result<Option<String>, AppError> {
        let expiration = time_span_ticks(expiry)
            .and_then(|ticks| utc_now().checked_add_signed(time_span_from_ticks(ticks)))
            .filter(|value| (1..=9999).contains(&value.year()))
            .ok_or_else(|| AppError::internal("阿里云 OSS 签名过期时间超出范围"))?;
        self.validate_operation(remote_path)?;
        let clock_offset = self.clock_offset.load(Ordering::Relaxed);
        let now = signing_time(clock_offset)?;
        let scope = format!(
            "{}/{}/oss/aliyun_v4_request",
            now.format("%Y%m%d"),
            self.settings.region
        );
        let mut parameters = BTreeMap::from([
            ("x-oss-signature-version", "OSS4-HMAC-SHA256".to_owned()),
            ("x-oss-date", now.format("%Y%m%dT%H%M%SZ").to_string()),
            (
                "x-oss-expires",
                (expiration - now).num_seconds().to_string(),
            ),
            (
                "x-oss-credential",
                format!("{}/{scope}", self.settings.access_key_id),
            ),
        ]);
        if is_inline {
            parameters.insert("response-content-disposition", "inline".to_owned());
        }
        let query = parameters
            .iter()
            .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
            .collect::<Vec<_>>()
            .join("&");
        let canonical = format!(
            "GET\n{}\n{query}\n\n\n{UNSIGNED_PAYLOAD}",
            self.canonical_uri(remote_path)
        );
        let (_, signature) = self.signature(now, &canonical)?;
        if expiration - utc_now() > chrono::TimeDelta::days(7) {
            return Err(AppError::internal("阿里云 OSS 临时地址有效期不能超过七天"));
        }
        let url = reqwest::Url::parse(&format!(
            "{}/{}?{query}&x-oss-signature={signature}",
            self.endpoint_url,
            encode_path(remote_path)
        ))
        .map_err(|_| AppError::internal("阿里云 OSS 临时地址无效"))?;
        Ok(Some(format!(
            "{}{}?{}",
            self.settings.url,
            url.path()
                .strip_prefix('/')
                .ok_or_else(|| AppError::internal("阿里云 OSS 临时地址路径无效"))?,
            url.query()
                .ok_or_else(|| AppError::internal("阿里云 OSS 临时地址缺少签名"))?
        )))
    }
}

/// 源 SDK 的名称规则为小写 ASCII 字母、数字和连字符。
fn valid_endpoint_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// DateTime 使用 100ns 精度，签名时刻与过期时刻分别读取。
fn utc_now() -> DateTime<Utc> {
    let now = Utc::now();
    now.with_nanosecond(now.nanosecond() / 100 * 100)
        .expect("截断后的纳秒仍在有效范围")
}

fn signing_time(offset: i64) -> Result<DateTime<Utc>, AppError> {
    chrono::TimeDelta::try_seconds(offset)
        .and_then(|offset| utc_now().checked_add_signed(offset))
        .filter(|value| (1..=9999).contains(&value.year()))
        .ok_or_else(|| AppError::internal("阿里云 OSS 签名时刻超出范围"))
}

/// 百分号编码对象键，保留路径分隔符。
fn encode_path(value: &str) -> String {
    value.split('/').map(encode).collect::<Vec<_>>().join("/")
}

/// UTF-8 RFC 3986 编码。
fn encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            output.push(*byte as char);
        } else {
            let _ = write!(output, "%{byte:02X}");
        }
    }
    output
}

/// HMAC SHA256 原始摘要，用于逐级派生密钥。
fn hmac_sha256(key: &[u8], value: &str) -> Result<Vec<u8>, AppError> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|_| AppError::internal("OSS 签名器初始化失败"))?;
    mac.update(value.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

/// 小写十六进制编码。
fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
