use http_client::HttpClientFactory;
use std::{borrow::Cow, collections::BTreeMap};

use async_trait::async_trait;
use common::app_error::AppError;
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sms::{AppSmsSender, SendSmsRequest};
use std::sync::Arc;
use uuid::Uuid;

/// ACS3 HMAC-SHA256 签名器类型
type HmacSha256 = Hmac<Sha256>;

/// 阿里云签名专用百分号编码字符集
const ALIYUN_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// 阿里云短信配置
#[derive(Clone, Deserialize)]
pub struct AliyunSmsSettings {
    /// 访问密钥标识
    #[serde(default)]
    pub access_key_id: String,

    /// 访问密钥内容
    #[serde(default)]
    pub access_key_secret: String,
}

impl Default for AliyunSmsSettings {
    /// 创建默认阿里云短信配置
    fn default() -> Self {
        Self {
            access_key_id: String::new(),
            access_key_secret: String::new(),
        }
    }
}

/// 阿里云短信发送器
pub struct AliyunSmsSender {
    /// 配置
    settings: AliyunSmsSettings,

    /// 网络客户端
    http_clients: Arc<HttpClientFactory>,
}

impl AliyunSmsSender {
    /// 创建阿里云短信发送器
    pub fn new(settings: AliyunSmsSettings, http_clients: Arc<HttpClientFactory>) -> Self {
        Self {
            settings,
            http_clients,
        }
    }

    /// 校验配置
    fn validate_settings(&self) -> Result<(), AppError> {
        if self.settings.access_key_id.trim().is_empty() {
            return Err(AppError::internal(
                "阿里云短信 AccessKeyId 未配置，请检查 [sms.aliyun].access_key_id",
            ));
        }

        if self.settings.access_key_secret.trim().is_empty() {
            return Err(AppError::internal(
                "阿里云短信 AccessKeySecret 未配置，请检查 [sms.aliyun].access_key_secret",
            ));
        }

        Ok(())
    }

    /// 构建请求参数
    fn build_query_parameters(
        &self,
        request: &SendSmsRequest,
    ) -> Result<BTreeMap<String, String>, AppError> {
        let template_param = serde_json::to_string(&request.template_params).map_err(|error| {
            AppError::internal(format!("阿里云短信模板参数序列化失败: {error}"))
        })?;

        let mut parameters = BTreeMap::new();
        if let Some(phone) = &request.phone {
            parameters.insert("PhoneNumbers".to_string(), phone.clone());
        }
        if let Some(sign_name) = &request.sign_name {
            parameters.insert("SignName".to_string(), sign_name.clone());
        }
        if let Some(template_code) = &request.template_code {
            parameters.insert("TemplateCode".to_string(), template_code.clone());
        }
        parameters.insert("TemplateParam".to_string(), template_param);
        Ok(parameters)
    }

    /// 构造 ACS3 请求；短信参数在查询串中，正文为空。
    fn build_request(&self, request: &SendSmsRequest) -> Result<reqwest::RequestBuilder, AppError> {
        let parameters = self.build_query_parameters(request)?;
        let query = parameters
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    aliyun_percent_encode(key),
                    aliyun_percent_encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let payload_hash = sha256_hex(b"");
        let headers = BTreeMap::from([
            ("host", default_aliyun_endpoint().to_owned()),
            ("x-acs-action", "SendSms".to_owned()),
            ("x-acs-content-sha256", payload_hash.clone()),
            ("x-acs-date", current_iso8601_timestamp()),
            ("x-acs-signature-nonce", Uuid::new_v4().to_string()),
            ("x-acs-version", "2017-05-25".to_owned()),
        ]);
        let canonical_headers = headers
            .iter()
            .map(|(key, value)| format!("{key}:{}\n", value.trim()))
            .collect::<String>();
        let signed_headers = headers.keys().copied().collect::<Vec<_>>().join(";");
        let canonical_request =
            format!("POST\n/\n{query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}");
        let string_to_sign = format!(
            "ACS3-HMAC-SHA256\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let mut mac = HmacSha256::new_from_slice(self.settings.access_key_secret.as_bytes())
            .map_err(|_| AppError::internal("阿里云短信签名器初始化失败"))?;
        mac.update(string_to_sign.as_bytes());
        let signature = hex_lower(mac.finalize().into_bytes().as_slice());
        let authorization = format!(
            "ACS3-HMAC-SHA256 Credential={},SignedHeaders={signed_headers},Signature={signature}",
            self.settings.access_key_id
        );
        let mut builder = self
            .http_clients
            .client()
            .post(format!("https://{}/?{query}", default_aliyun_endpoint()))
            .header("accept", "application/json")
            .header("authorization", authorization);
        for (key, value) in headers {
            builder = builder.header(key, value);
        }
        Ok(builder)
    }

    /// 请求结束即完成源组件调用，不检查业务 Code。
    async fn send_request(&self, request: SendSmsRequest) -> Result<(), AppError> {
        self.validate_settings()?;
        let response = self
            .build_request(&request)?
            .send()
            .await
            .map_err(|_| AppError::internal("阿里云短信请求失败"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::internal(format!(
                "阿里云短信 HTTP 请求失败: {status}"
            )));
        }
        let _: AliyunSmsResponse = response
            .json()
            .await
            .map_err(|_| AppError::internal("阿里云短信响应格式无效"))?;
        Ok(())
    }
}

#[async_trait]
impl AppSmsSender for AliyunSmsSender {
    /// 与源组件 catch 一致，发送异常返回 false。
    async fn send_sms(&self, request: SendSmsRequest) -> Result<bool, AppError> {
        Ok(self.send_request(request).await.is_ok())
    }
}

/// SDK 响应字段允许缺失；业务 Code 不决定此组件的返回值。
#[derive(Deserialize)]
struct AliyunSmsResponse {
    #[serde(rename = "RequestId")]
    _request_id: Option<String>,
    #[serde(rename = "BizId")]
    _biz_id: Option<String>,
    #[serde(rename = "Code")]
    _code: Option<String>,
    #[serde(rename = "Message")]
    _message: Option<String>,
}

/// 阿里云短信端点。
fn default_aliyun_endpoint() -> &'static str {
    "dysmsapi.aliyuncs.com"
}

/// 构建当前 ISO8601 UTC 时间。
fn current_iso8601_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// RFC 3986 查询参数编码。
fn aliyun_percent_encode(value: &str) -> Cow<'_, str> {
    utf8_percent_encode(value, ALIYUN_ENCODE_SET).into()
}

/// SHA256 十六进制摘要。
fn sha256_hex(value: &[u8]) -> String {
    hex_lower(&Sha256::digest(value))
}

/// 十六进制小写编码。
fn hex_lower(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}
