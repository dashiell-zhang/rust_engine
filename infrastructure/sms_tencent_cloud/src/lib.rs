use async_trait::async_trait;
use common::app_error::AppError;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{
    Client,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sms::{AppSmsSender, SendSmsRequest};

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
    client: Client,
}

impl TencentSmsSender {
    /// 创建腾讯云短信发送器
    pub fn new(settings: TencentSmsSettings) -> Self {
        Self {
            settings,
            client: Client::new(),
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
        let template_id = request.template_code.trim();
        let sign_name = request.sign_name.trim();

        if template_id.is_empty() {
            return Err(AppError::bad_request("腾讯云短信 template_code 不可以空"));
        }

        if sign_name.is_empty() {
            return Err(AppError::bad_request("腾讯云短信 sign_name 不可以空"));
        }

        let body = TencentSmsRequestBody {
            phone_number_set: vec![normalize_tencent_phone(&request.phone)],
            sms_sdk_app_id: self.settings.app_id.trim().to_string(),
            sign_name: sign_name.to_string(),
            template_id: template_id.to_string(),
            template_param_set: normalize_tencent_template_params(&request.template_params),
        };

        serde_json::to_string(&body)
            .map_err(|error| AppError::internal(format!("腾讯云短信请求体序列化失败: {error}")))
    }

    /// 计算签名
    fn build_authorization(&self, timestamp: i64, body: &str) -> Result<String, AppError> {
        let host = default_tencent_endpoint().to_ascii_lowercase();
        let date = current_utc_date();
        let credential_scope = format!("{date}/sms/tc3_request");
        let hashed_body = sha256_hex(body.as_bytes());
        let canonical_request = format!(
            "POST\n/\n\ncontent-type:application/json; charset=utf-8\nhost:{host}\n\ncontent-type;host\n{hashed_body}"
        );
        let hashed_canonical_request = sha256_hex(canonical_request.as_bytes());
        let string_to_sign =
            format!("TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{hashed_canonical_request}");

        let secret_date = hmac_sha256(
            format!("TC3{}", self.settings.secret_key.trim()).as_bytes(),
            date.as_bytes(),
        )?;
        let secret_service = hmac_sha256(&secret_date, b"sms")?;
        let secret_signing = hmac_sha256(&secret_service, b"tc3_request")?;
        let signature = hex_lower(&hmac_sha256(&secret_signing, string_to_sign.as_bytes())?);

        Ok(format!(
            "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders=content-type;host, Signature={}",
            self.settings.secret_id.trim(),
            credential_scope,
            signature
        ))
    }
}

#[async_trait]
impl AppSmsSender for TencentSmsSender {
    /// 发送短信
    async fn send_sms(&self, request: SendSmsRequest) -> Result<(), AppError> {
        self.validate_settings()?;
        let body = self.build_request_body(&request)?;
        let timestamp = current_unix_timestamp();
        let authorization = self.build_authorization(timestamp, &body)?;

        let response = self
            .client
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

        if !status.is_success() {
            return Err(AppError::internal(format!(
                "腾讯云短信请求失败: {status}, body: {body}"
            )));
        }

        let payload =
            serde_json::from_str::<TencentSmsResponseEnvelope>(&body).map_err(|error| {
                AppError::internal(format!("腾讯云短信响应解析失败: {error}, body: {body}"))
            })?;

        if let Some(error) = payload.response.error {
            return Err(AppError::internal(format!(
                "腾讯云短信发送失败: code={}, message={}, request_id={}",
                error.code, error.message, payload.response.request_id
            )));
        }

        let failed_status = payload
            .response
            .send_status_set
            .iter()
            .find(|item| !item.code.eq_ignore_ascii_case("Ok"));

        if let Some(failed_status) = failed_status {
            return Err(AppError::internal(format!(
                "腾讯云短信发送失败: code={}, message={}, phone={}, request_id={}",
                failed_status.code,
                failed_status.message,
                failed_status.phone_number,
                payload.response.request_id
            )));
        }

        tracing::info!(
            target: "sms.tencent.send",
            phone = %request.phone,
            request_id = %payload.response.request_id,
            "腾讯云短信发送成功"
        );

        Ok(())
    }
}

/// 腾讯云短信请求体
#[derive(Serialize)]
struct TencentSmsRequestBody {
    /// 手机号列表
    #[serde(rename = "PhoneNumberSet")]
    phone_number_set: Vec<String>,

    /// 短信应用标识
    #[serde(rename = "SmsSdkAppId")]
    sms_sdk_app_id: String,

    /// 短信签名
    #[serde(rename = "SignName")]
    sign_name: String,

    /// 模板 ID
    #[serde(rename = "TemplateId")]
    template_id: String,

    /// 模板参数列表
    #[serde(rename = "TemplateParamSet")]
    template_param_set: Vec<String>,
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
    /// 请求标识
    #[serde(rename = "RequestId")]
    request_id: String,

    /// 发送状态列表
    #[serde(rename = "SendStatusSet", default)]
    send_status_set: Vec<TencentSmsSendStatus>,

    /// 错误信息
    #[serde(rename = "Error")]
    error: Option<TencentSmsError>,
}

/// 腾讯云短信发送状态
#[derive(Deserialize)]
struct TencentSmsSendStatus {
    /// 发送状态码
    #[serde(rename = "Code")]
    code: String,

    /// 发送说明
    #[serde(rename = "Message")]
    message: String,

    /// 号码
    #[serde(rename = "PhoneNumber")]
    phone_number: String,
}

/// 腾讯云短信错误
#[derive(Deserialize)]
struct TencentSmsError {
    /// 错误码
    #[serde(rename = "Code")]
    code: String,

    /// 错误消息
    #[serde(rename = "Message")]
    message: String,
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

/// 构建当前 UTC 日期
fn current_utc_date() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// 构建当前 Unix 时间戳
fn current_unix_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

/// 归一化腾讯云手机号
fn normalize_tencent_phone(phone: &str) -> String {
    let phone = phone.trim();

    if phone.starts_with('+') {
        phone.to_string()
    } else if phone.chars().all(|item| item.is_ascii_digit()) && phone.len() == 11 {
        format!("+86{phone}")
    } else {
        format!("+{phone}")
    }
}

/// 归一化腾讯云模板参数
fn normalize_tencent_template_params(
    template_params: &indexmap::IndexMap<String, String>,
) -> Vec<String> {
    template_params.values().cloned().collect()
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
