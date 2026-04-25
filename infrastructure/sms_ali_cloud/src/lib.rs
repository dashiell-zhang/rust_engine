use std::{borrow::Cow, collections::BTreeMap};

use async_trait::async_trait;
use common::app_error::AppError;
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::Client;
use serde::Deserialize;
use sha1::Sha1;
use sms::{AppSmsSender, SendSmsRequest};
use uuid::Uuid;

/// HMAC-SHA1 签名器类型
type HmacSha1 = Hmac<Sha1>;

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
    client: Client,
}

impl AliyunSmsSender {
    /// 创建阿里云短信发送器
    pub fn new(settings: AliyunSmsSettings) -> Self {
        Self {
            settings,
            client: Client::new(),
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
        let sign_name = request.sign_name.trim();
        let template_code = request.template_code.trim();

        if sign_name.is_empty() {
            return Err(AppError::bad_request("阿里云短信 sign_name 不可以空"));
        }

        if template_code.is_empty() {
            return Err(AppError::bad_request("阿里云短信 template_code 不可以空"));
        }

        let template_param = serde_json::to_string(&request.template_params).map_err(|error| {
            AppError::internal(format!("阿里云短信模板参数序列化失败: {error}"))
        })?;

        let mut parameters = BTreeMap::new();
        parameters.insert(
            "AccessKeyId".to_string(),
            self.settings.access_key_id.trim().to_string(),
        );
        parameters.insert("Action".to_string(), "SendSms".to_string());
        parameters.insert("Format".to_string(), "JSON".to_string());
        parameters.insert("OutId".to_string(), Uuid::new_v4().to_string());
        parameters.insert("SignatureMethod".to_string(), "HMAC-SHA1".to_string());
        parameters.insert("SignatureNonce".to_string(), Uuid::new_v4().to_string());
        parameters.insert("SignatureVersion".to_string(), "1.0".to_string());
        parameters.insert("Timestamp".to_string(), current_iso8601_timestamp());
        parameters.insert(
            "PhoneNumbers".to_string(),
            normalize_phone_without_plus(&request.phone),
        );
        parameters.insert("Version".to_string(), default_aliyun_version().to_string());
        parameters.insert("SignName".to_string(), sign_name.to_string());
        parameters.insert("TemplateCode".to_string(), template_code.to_string());
        parameters.insert("TemplateParam".to_string(), template_param);
        parameters.insert(
            "RegionId".to_string(),
            default_aliyun_region_id().to_string(),
        );

        Ok(parameters)
    }

    /// 计算签名
    fn sign_query_parameters(
        &self,
        parameters: &BTreeMap<String, String>,
    ) -> Result<String, AppError> {
        let canonical_query = parameters
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
        let string_to_sign = format!("GET&%2F&{}", aliyun_percent_encode(&canonical_query));
        let signing_key = format!("{}&", self.settings.access_key_secret.trim());
        let mut mac = HmacSha1::new_from_slice(signing_key.as_bytes())
            .map_err(|error| AppError::internal(format!("阿里云短信签名器初始化失败: {error}")))?;
        mac.update(string_to_sign.as_bytes());
        Ok(base64_simple(mac.finalize().into_bytes().as_slice()))
    }
}

#[async_trait]
impl AppSmsSender for AliyunSmsSender {
    /// 发送短信
    async fn send_sms(&self, request: SendSmsRequest) -> Result<(), AppError> {
        self.validate_settings()?;
        let mut parameters = self.build_query_parameters(&request)?;
        let signature = self.sign_query_parameters(&parameters)?;
        parameters.insert("Signature".to_string(), signature);

        let response = self
            .client
            .get(format!("https://{}/", default_aliyun_endpoint()))
            .query(&parameters)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("阿里云短信请求失败: {error}")))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("阿里云短信响应读取失败: {error}")))?;

        if !status.is_success() {
            return Err(AppError::internal(format!(
                "阿里云短信请求失败: {status}, body: {body}"
            )));
        }

        let payload = serde_json::from_str::<AliyunSmsResponse>(&body).map_err(|error| {
            AppError::internal(format!("阿里云短信响应解析失败: {error}, body: {body}"))
        })?;

        if payload.code.eq_ignore_ascii_case("OK") {
            tracing::info!(
                target: "sms.aliyun.send",
                phone = %request.phone,
                request_id = %payload.request_id,
                biz_id = ?payload.biz_id,
                "阿里云短信发送成功"
            );
            Ok(())
        } else {
            Err(AppError::internal(format!(
                "阿里云短信发送失败: code={}, message={}, request_id={}",
                payload.code, payload.message, payload.request_id
            )))
        }
    }
}

/// 阿里云短信响应
#[derive(Deserialize)]
struct AliyunSmsResponse {
    /// 请求标识
    #[serde(rename = "RequestId")]
    request_id: String,

    /// 业务流水号
    #[serde(rename = "BizId")]
    biz_id: Option<String>,

    /// 响应码
    #[serde(rename = "Code")]
    code: String,

    /// 响应信息
    #[serde(rename = "Message")]
    message: String,
}

/// 获取默认阿里云 endpoint
fn default_aliyun_endpoint() -> String {
    "dysmsapi.aliyuncs.com".to_string()
}

/// 获取默认阿里云版本
fn default_aliyun_version() -> String {
    "2017-05-25".to_string()
}

/// 获取默认阿里云区域
fn default_aliyun_region_id() -> String {
    "cn-hangzhou".to_string()
}

/// 构建当前 ISO8601 时间
fn current_iso8601_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// 阿里云地址编码
fn aliyun_percent_encode(value: &str) -> Cow<'_, str> {
    utf8_percent_encode(value, ALIYUN_ENCODE_SET).into()
}

/// 归一化手机号为无加号格式
fn normalize_phone_without_plus(phone: &str) -> String {
    let phone = phone.trim();

    if let Some(phone) = phone.strip_prefix("+86") {
        phone.to_string()
    } else if let Some(phone) = phone.strip_prefix('+') {
        phone.to_string()
    } else {
        phone.to_string()
    }
}

/// Base64 编码
fn base64_simple(value: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    let mut index = 0;

    while index < value.len() {
        let first = value[index];
        let second = value.get(index + 1).copied();
        let third = value.get(index + 2).copied();

        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(
            TABLE[(((first & 0b0000_0011) << 4) | second.unwrap_or(0) >> 4) as usize] as char,
        );

        match second {
            Some(second) => {
                output.push(
                    TABLE[(((second & 0b0000_1111) << 2) | third.unwrap_or(0) >> 6) as usize]
                        as char,
                );
            }
            None => output.push('='),
        }

        match third {
            Some(third) => output.push(TABLE[(third & 0b0011_1111) as usize] as char),
            None => output.push('='),
        }

        index += 3;
    }

    output
}
