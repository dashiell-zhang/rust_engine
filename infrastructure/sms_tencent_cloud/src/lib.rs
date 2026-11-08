use async_trait::async_trait;
use common::app_error::AppError;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sms::{AppSmsSender, HttpClientFactory, SendSmsRequest};
use std::sync::Arc;

/// HMAC-SHA256 签名器类型
type HmacSha256 = Hmac<Sha256>;

/// 腾讯云短信配置
#[derive(Clone, Deserialize)]
pub struct TencentSmsSettings {
    /// 短信应用标识
    #[serde(default)]
    pub app_id: String,

    /// 密钥标识
    #[serde(default)]
    pub secret_id: String,

    /// 密钥内容
    #[serde(default)]
    pub secret_key: String,
}

impl Default for TencentSmsSettings {
    /// 创建默认腾讯云短信配置
    fn default() -> Self {
        Self {
            app_id: String::new(),
            secret_id: String::new(),
            secret_key: String::new(),
        }
    }
}

/// 腾讯云短信发送器
pub struct TencentSmsSender {
    /// 配置
    settings: TencentSmsSettings,

    /// 网络客户端
    http_clients: Arc<HttpClientFactory>,
}

impl TencentSmsSender {
    /// 创建腾讯云短信发送器
    pub fn new(settings: TencentSmsSettings, http_clients: Arc<HttpClientFactory>) -> Self {
        Self {
            settings,
            http_clients,
        }
    }

    /// 校验配置
    fn validate_settings(&self) -> Result<(), AppError> {
        if self.settings.secret_id.trim().is_empty() {
            return Err(AppError::internal(
                "腾讯云短信 secret_id 未配置，请检查 [sms.tencent].secret_id",
            ));
        }

        if self.settings.secret_key.trim().is_empty() {
            return Err(AppError::internal(
                "腾讯云短信 secret_key 未配置，请检查 [sms.tencent].secret_key",
            ));
        }

        if self.settings.app_id.trim().is_empty() {
            return Err(AppError::internal(
                "腾讯云短信 app_id 未配置，请检查 [sms.tencent].app_id",
            ));
        }

        Ok(())
    }

    /// 构建请求体
    fn build_request_body(&self, request: &SendSmsRequest) -> Result<String, AppError> {
        let template_params = request
            .template_params
            .as_ref()
            .ok_or_else(|| AppError::internal("腾讯云短信模板参数为空"))?;
        let body = TencentSmsRequestBody {
            phone_number_set: vec![format!(
                "+86{}",
                request.phone.as_deref().unwrap_or_default()
            )],
            sms_sdk_app_id: self.settings.app_id.clone(),
            sign_name: request.sign_name.as_deref(),
            template_id: request.template_code.as_deref(),
            template_param_set: template_params.values().map(Option::as_deref).collect(),
        };

        serde_json::to_string(&body)
            .map_err(|error| AppError::internal(format!("腾讯云短信请求体序列化失败: {error}")))
    }

    /// 计算签名
    fn build_authorization(&self, timestamp: i64, body: &str) -> Result<String, AppError> {
        let host = default_tencent_endpoint().to_ascii_lowercase();
        let date = chrono::DateTime::from_timestamp(timestamp, 0)
            .ok_or_else(|| AppError::internal("腾讯云签名时间无效"))?
            .format("%Y-%m-%d")
            .to_string();
        let credential_scope = format!("{date}/sms/tc3_request");
        let hashed_body = sha256_hex(body.as_bytes());
        let canonical_request = format!(
            "POST\n/\n\ncontent-type:application/json; charset=utf-8\nhost:{host}\n\ncontent-type;host\n{hashed_body}"
        );
        let hashed_canonical_request = sha256_hex(canonical_request.as_bytes());
        let string_to_sign =
            format!("TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{hashed_canonical_request}");

        let secret_date = hmac_sha256(
            format!("TC3{}", self.settings.secret_key).as_bytes(),
            date.as_bytes(),
        )?;
        let secret_service = hmac_sha256(&secret_date, b"sms")?;
        let secret_signing = hmac_sha256(&secret_service, b"tc3_request")?;
        let signature = hex_lower(&hmac_sha256(&secret_signing, string_to_sign.as_bytes())?);

        Ok(format!(
            "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders=content-type;host, Signature={}",
            self.settings.secret_id, credential_scope, signature
        ))
    }
}

impl TencentSmsSender {
    /// 执行一次 SDK 等价请求；提供商边界将异常转换为 false。
    async fn send_request(&self, request: SendSmsRequest) -> Result<(), AppError> {
        self.validate_settings()?;
        let body = self.build_request_body(&request)?;
        let timestamp = current_unix_timestamp();
        let authorization = self.build_authorization(timestamp, &body)?;

        let response = self
            .http_clients
            .client()
            .post(format!("https://{}/", default_tencent_endpoint()))
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .header("Host", default_tencent_endpoint())
            .header("X-TC-Action", "SendSms")
            .header("X-TC-Version", default_tencent_version())
            .header("X-TC-Region", default_tencent_region())
            .header("X-TC-Timestamp", timestamp.to_string())
            .header(AUTHORIZATION, authorization)
            .body(body)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("腾讯云短信请求失败: {error}")))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("腾讯云短信响应读取失败: {error}")))?;

        if status != reqwest::StatusCode::OK {
            return Err(AppError::internal(format!(
                "腾讯云短信 HTTP 请求失败: {status}"
            )));
        }
        let payload: TencentSmsResponseEnvelope = serde_json::from_str(&body)
            .map_err(|_| AppError::internal("腾讯云短信响应格式无效"))?;
        if payload.response.error.is_some() {
            return Err(AppError::internal("腾讯云短信 API 返回错误"));
        }
        // 源代码不读取 SendStatusSet；号码级业务失败不转换为异常。
        Ok(())
    }
}

#[async_trait]
impl AppSmsSender for TencentSmsSender {
    async fn send_sms(&self, request: SendSmsRequest) -> Result<bool, AppError> {
        Ok(self.send_request(request).await.is_ok())
    }
}

/// 腾讯云短信请求体
#[derive(Serialize)]
struct TencentSmsRequestBody<'a> {
    /// 手机号列表
    #[serde(rename = "PhoneNumberSet")]
    phone_number_set: Vec<String>,

    /// 短信应用标识
    #[serde(rename = "SmsSdkAppid")]
    sms_sdk_app_id: String,

    /// 短信签名
    #[serde(rename = "Sign", skip_serializing_if = "Option::is_none")]
    sign_name: Option<&'a str>,

    /// 模板 ID
    #[serde(rename = "TemplateID", skip_serializing_if = "Option::is_none")]
    template_id: Option<&'a str>,

    /// 模板参数列表
    #[serde(rename = "TemplateParamSet")]
    template_param_set: Vec<Option<&'a str>>,
}

/// 腾讯云短信响应包装
#[derive(Deserialize)]
struct TencentSmsResponseEnvelope {
    /// 响应体
    #[serde(rename = "Response")]
    response: TencentSmsResponse,
}

/// 腾讯云短信响应
#[derive(Deserialize)]
struct TencentSmsResponse {
    /// API 错误对象；号码级 SendStatusSet 不参与此判断。
    #[serde(rename = "Error")]
    error: Option<serde_json::Value>,
}

/// 获取默认腾讯云 endpoint
fn default_tencent_endpoint() -> String {
    "sms.tencentcloudapi.com".to_string()
}

/// 获取默认腾讯云区域
fn default_tencent_region() -> String {
    "ap-guangzhou".to_string()
}

/// 获取默认腾讯云版本
fn default_tencent_version() -> String {
    "2019-07-11".to_string()
}

/// 构建当前 Unix 时间戳
fn current_unix_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

/// 计算 HMAC SHA256 摘要
fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|error| AppError::internal(format!("腾讯云短信签名器初始化失败: {error}")))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// 计算 SHA256 十六进制摘要
fn sha256_hex(value: &[u8]) -> String {
    hex_lower(&Sha256::digest(value))
}

/// 十六进制小写编码
fn hex_lower(value: &[u8]) -> String {
    value.iter().map(|item| format!("{item:02x}")).collect()
}
