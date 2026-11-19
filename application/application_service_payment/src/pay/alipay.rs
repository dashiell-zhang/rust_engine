//! 支付宝网关和 RSA2 通知协议。
use super::service::PaymentNotification;
use application_interface::error::AppError;
use chrono::{FixedOffset, NaiveDateTime};
use sea_orm::prelude::Decimal;
use serde_json::Value;
use std::collections::HashMap;
use url::form_urlencoded;
/// 支付宝网关地址
pub(super) const ALIPAY_GATEWAY_URL: &str = "https://openapi.alipay.com/gateway.do";

/// 对齐 NetEngine 当前使用的 SDK 协议元数据，不表示运行时依赖 .NET SDK。
const ALIPAY_SDK_PROTOCOL: &str = "alipay-sdk-net-4.9.1424.ALL";

/// 支付宝网页支付配置
pub(super) struct AliPayWebSettings {
    /// 应用标识
    pub(super) app_id: String,

    /// 应用私钥
    pub(super) app_private_key: String,

    /// 支付宝公钥
    pub(super) alipay_public_key: String,

    /// 网关地址
    pub(super) gateway_url: String,
}

/// 执行支付宝开放接口调用
///
/// 以表单方式调用支付宝网关并返回原始 JSON 包装体
pub(super) async fn execute_alipay_api(
    client: reqwest::Client,
    settings: &AliPayWebSettings,
    method: &str,
    biz_content: Value,
    notify_url: Option<&str>,
    charset: &str,
) -> Result<String, AppError> {
    let mut parameters = vec![
        ("alipay_sdk".to_string(), ALIPAY_SDK_PROTOCOL.to_string()),
        ("app_id".to_string(), settings.app_id.clone()),
        ("biz_content".to_string(), biz_content.to_string()),
        ("charset".to_string(), charset.to_string()),
        ("format".to_string(), "json".to_string()),
        ("method".to_string(), method.to_string()),
        ("sign_type".to_string(), "RSA2".to_string()),
        (
            "timestamp".to_string(),
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        ),
        ("version".to_string(), "1.0".to_string()),
    ];

    if let Some(notify_url) = notify_url.filter(|value| !value.is_empty()) {
        parameters.push(("notify_url".to_string(), notify_url.to_string()));
    }

    // 与网页支付相同，先按字典序拼接待签名内容
    parameters.retain(|(_, value)| !value.is_empty());
    parameters.sort_by(|left, right| left.0.cmp(&right.0));
    let content = build_sign_content(&parameters);
    let sign = sign_alipay_content(&settings.app_private_key, &content)?;
    parameters.push(("sign".to_string(), sign));

    let body = parameters
        .iter()
        .fold(
            form_urlencoded::Serializer::new(String::new()),
            |mut serializer, (key, value)| {
                serializer.append_pair(key, value);
                serializer
            },
        )
        .finish();

    let response = client
        .post(&settings.gateway_url)
        .query(&[("charset", charset)])
        .header(
            reqwest::header::CONTENT_TYPE,
            format!("application/x-www-form-urlencoded;charset={charset}"),
        )
        .body(body)
        .send()
        .await
        .map_err(|error| AppError::internal(format!("调用支付宝接口失败: {error}")))?;

    let response_text = response
        .text()
        .await
        .map_err(|error| AppError::internal(format!("读取支付宝接口响应失败: {error}")))?;

    super::alipay_response::parse_response(&response_text, method, &settings.alipay_public_key)
}

/// 组装支付宝网关地址
///
/// 按支付宝要求拼接公共参数并完成 RSA2 签名
pub(super) fn build_alipay_gateway_url(
    settings: &AliPayWebSettings,
    method: &str,
    biz_content: Value,
    notify_url: &str,
    return_url: Option<&str>,
) -> Result<String, AppError> {
    let mut parameters = vec![
        ("alipay_sdk".to_string(), ALIPAY_SDK_PROTOCOL.to_string()),
        ("app_id".to_string(), settings.app_id.clone()),
        ("biz_content".to_string(), biz_content.to_string()),
        ("charset".to_string(), "UTF-8".to_string()),
        ("format".to_string(), "json".to_string()),
        ("method".to_string(), method.to_string()),
        ("notify_url".to_string(), notify_url.to_string()),
        ("sign_type".to_string(), "RSA2".to_string()),
        (
            "timestamp".to_string(),
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        ),
        ("version".to_string(), "1.0".to_string()),
    ];

    if let Some(value) = return_url.filter(|value| !value.is_empty()) {
        parameters.push(("return_url".to_string(), value.to_string()));
    }

    parameters.retain(|(_, value)| !value.is_empty());
    parameters.sort_by(|left, right| left.0.cmp(&right.0));

    let content = build_sign_content(&parameters);
    let sign = sign_alipay_content(&settings.app_private_key, &content)?;
    parameters.push(("sign".to_string(), sign));

    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in parameters {
        serializer.append_pair(&key, &value);
    }

    Ok(format!("{}?{}", settings.gateway_url, serializer.finish()))
}
/// 构建支付宝签名明文
fn build_sign_content(parameters: &[(String, String)]) -> String {
    parameters
        .iter()
        .filter(|(key, value)| key != "sign" && !value.is_empty())
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// 对支付宝参数进行签名
fn sign_alipay_content(private_key_pem: &str, content: &str) -> Result<String, AppError> {
    use common::crypto_helper::{
        RsaHashAlgorithm, RsaSignaturePadding, StringEncoding, rsa_sign_data,
    };
    rsa_sign_data(
        private_key_pem,
        content,
        StringEncoding::Base64,
        RsaHashAlgorithm::Sha256,
        RsaSignaturePadding::Pkcs1,
    )
    .map_err(|error| AppError::internal(format!("支付宝 RSA2 签名失败: {error}")))
}

/// 校验支付宝异步通知签名
fn verify_alipay_notify_signature(
    parameters: &HashMap<String, String>,
    public_key_pem: &str,
) -> Result<bool, AppError> {
    use common::crypto_helper::{
        RsaHashAlgorithm, RsaSignaturePadding, StringEncoding, rsa_verify_data,
    };
    let signature_text = parameters
        .get("sign")
        .map(String::as_str)
        .ok_or_else(|| AppError::internal("支付宝通知缺少签名"))?;

    let mut pairs = parameters
        .iter()
        .filter(|(key, value)| *key != "sign" && *key != "sign_type" && !value.is_empty())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();

    pairs.sort_by(|left, right| left.0.cmp(&right.0));
    let content = build_sign_content(&pairs);

    rsa_verify_data(
        public_key_pem,
        &content,
        signature_text,
        StringEncoding::Base64,
        RsaHashAlgorithm::Sha256,
        RsaSignaturePadding::Pkcs1,
    )
    .map_err(|_| AppError::internal("支付宝通知签名或公钥格式无效"))
}

pub(super) fn read_notification(
    parameters: &HashMap<String, String>,
    settings: &AliPayWebSettings,
) -> Result<PaymentNotification, AppError> {
    let field = |key: &str| {
        parameters
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| AppError::bad_request(format!("支付宝通知缺少 {key}")))
    };
    if field("sign_type")? != "RSA2"
        || !verify_alipay_notify_signature(parameters, &settings.alipay_public_key)?
        || !matches!(field("trade_status")?, "TRADE_SUCCESS" | "TRADE_FINISHED")
    {
        return Err(AppError::bad_request("支付宝支付通知无效"));
    }
    let raw = field("total_amount")?;
    if !raw.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return Err(AppError::bad_request("支付宝支付通知无效"));
    }
    let amount = Decimal::from_str_exact(raw)
        .map_err(|_| AppError::bad_request("支付宝通知金额格式无效"))?;
    let time = field("gmt_payment")?;
    let naive = NaiveDateTime::parse_from_str(time, "%Y-%m-%d %H:%M:%S")
        .map_err(|_| AppError::bad_request("支付宝支付时间无效"))?;
    if naive.format("%Y-%m-%d %H:%M:%S").to_string() != time || amount <= Decimal::ZERO {
        return Err(AppError::bad_request("支付宝支付通知无效"));
    }
    let paid_at = naive
        .and_local_timezone(FixedOffset::east_opt(8 * 3600).unwrap())
        .single()
        .unwrap()
        .to_utc();
    Ok(PaymentNotification {
        order_no: field("out_trade_no")?.into(),
        transaction_id: field("trade_no")?.into(),
        amount,
        paid_at,
    })
}
