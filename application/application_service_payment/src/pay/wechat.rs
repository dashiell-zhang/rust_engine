//! 微信支付下单、平台证书和交易通知协议。
use super::service::PaymentNotification;
use application_interface::error::AppError;
use application_model::pay::models::{WxPayAppDto, WxPayJsapiDto};
use base64::Engine;
use chrono::Utc;
use distributed_cache::{CacheEntryOptions, DistributedCache, DistributedCacheExt};
use distributed_lock::DistributedLock;
use sea_orm::prelude::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};

pub(super) struct WxClient<'a> {
    pub http: reqwest::Client,
    pub cache: &'a dyn DistributedCache,
    pub lock: &'a dyn DistributedLock,
}
/// 微信平台证书缓存秒数
const WX_PAY_CERT_CACHE_TTL_SECONDS: u64 = 3600;

/// 微信支付 API 基地址
const WX_PAY_BASE_URL: &str = "https://api.mch.weixin.qq.com";

/// 微信支付传输与回调配置
pub(super) struct WxPaySettings {
    /// 应用标识
    pub(super) app_id: String,

    /// 商户号
    pub(super) mch_id: String,

    /// API 证书序列号
    pub(super) mch_api_cert_id: String,

    /// API 证书私钥（PKCS8 PEM）
    pub(super) mch_api_cert_key: String,

    /// API V3 密钥
    pub(super) mch_api_v3_key: String,
}

/// 微信支付平台证书条目
#[derive(Debug, Deserialize, Serialize)]
struct WxPayCertEntry {
    /// 证书序列号
    serial_no: String,

    /// 加密证书信息
    encrypt_certificate: WxPayEncryptedCert,
}

#[derive(Serialize, Deserialize)]
struct PlatformCertificate {
    serial_no: String,
    certificate: String,
}

/// 微信支付加密证书信息
#[derive(Debug, Deserialize, Serialize)]
struct WxPayEncryptedCert {
    #[serde(default)]
    algorithm: Option<String>,

    /// 密文
    ciphertext: String,

    /// 附加数据
    associated_data: Option<String>,

    /// 随机串
    nonce: String,
}

/// 微信支付平台证书列表
#[derive(Debug, Deserialize, Serialize)]
struct WxPayCertificatesResponse {
    /// 证书列表
    data: Vec<WxPayCertEntry>,
}

impl WxClient<'_> {
    async fn transaction(
        &self,
        settings: &WxPaySettings,
        order: &PrepayOrder<'_>,
        scene: &str,
        extra: Option<(&str, Value)>,
        field: &str,
    ) -> Result<String, AppError> {
        let mut data = json!({"mchid":settings.mch_id,"appid":settings.app_id,"out_trade_no":order.number,
            "description":order.title,"notify_url":order.notify_url,"amount":{"total":decimal_to_fen(order.amount)?,"currency":"CNY"}});
        if let Some((key, value)) = extra {
            data[key] = value;
        }
        let body = self
            .wx_pay_http_post(
                settings,
                &format!("{WX_PAY_BASE_URL}/v3/pay/transactions/{scene}"),
                &data,
            )
            .await?;
        let response: Value = serde_json::from_str(&body)
            .map_err(|_| AppError::bad_request("微信支付下单响应格式无效"))?;
        response[field]
            .as_str()
            .filter(|v| !v.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| AppError::bad_request("微信支付下单结果不完整"))
    }
    pub(super) async fn jsapi(
        &self,
        settings: &WxPaySettings,
        order: &PrepayOrder<'_>,
        open_id: &str,
    ) -> Result<WxPayJsapiDto, AppError> {
        let prepay_id = self
            .transaction(
                settings,
                order,
                "jsapi",
                Some(("payer", json!({"openid":open_id}))),
                "prepay_id",
            )
            .await?;
        let package = format!("prepay_id={}", prepay_id);
        let time_stamp = Utc::now().timestamp();
        let nonce_str = generate_nonce();
        let message = format!(
            "{}\n{}\n{}\n{}\n",
            settings.app_id, time_stamp, nonce_str, package
        );
        let sign = wx_pay_rsa_sign(&settings.mch_api_cert_key, &message)?;

        Ok(WxPayJsapiDto {
            app_id: settings.app_id.clone(),
            package,
            nonce_str,
            time_stamp: time_stamp.to_string(),
            sign,
        })
    }
    pub(super) async fn app(
        &self,
        settings: &WxPaySettings,
        order: &PrepayOrder<'_>,
    ) -> Result<WxPayAppDto, AppError> {
        let prepay_id = self
            .transaction(settings, order, "app", None, "prepay_id")
            .await?;
        let time_stamp = Utc::now().timestamp();
        let nonce_str = generate_nonce();
        let message = format!(
            "{}\n{}\n{}\n{}\n",
            settings.app_id, time_stamp, nonce_str, prepay_id
        );
        let sign = wx_pay_rsa_sign(&settings.mch_api_cert_key, &message)?;

        Ok(WxPayAppDto {
            app_id: settings.app_id.clone(),
            partner_id: settings.mch_id.clone(),
            prepay_id,
            nonce_str,
            time_stamp: time_stamp.to_string(),
            sign,
        })
    }
    pub(super) async fn h5(
        &self,
        settings: &WxPaySettings,
        order: &PrepayOrder<'_>,
        client_ip: &str,
    ) -> Result<String, AppError> {
        self.transaction(
            settings,
            order,
            "h5",
            Some((
                "scene_info",
                json!({"payer_client_ip":client_ip,"h5_info":{"type":"Wap"}}),
            )),
            "h5_url",
        )
        .await
    }
    pub(super) async fn pc(
        &self,
        settings: &WxPaySettings,
        order: &PrepayOrder<'_>,
    ) -> Result<String, AppError> {
        self.transaction(settings, order, "native", None, "code_url")
            .await
    }
    pub(super) async fn read_notification(
        &self,
        settings: &WxPaySettings,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<PaymentNotification, AppError> {
        self.verify_wx(settings, headers, body).await?;
        let notice: Value =
            serde_json::from_str(body).map_err(|_| AppError::bad_request("微信通知格式无效"))?;
        if notice["event_type"] != "TRANSACTION.SUCCESS"
            || notice["resource_type"] != "encrypt-resource"
        {
            return Err(AppError::bad_request("微信支付通知无效"));
        }
        let resource: WxPayEncryptedCert = serde_json::from_value(notice["resource"].clone())
            .map_err(|_| AppError::bad_request("微信加密资源无效"))?;
        let plaintext = decrypt_resource(&resource, &settings.mch_api_v3_key)?;
        let p: Value = serde_json::from_str(&plaintext)
            .map_err(|_| AppError::bad_request("微信交易通知格式无效"))?;
        if p["appid"] != settings.app_id
            || p["mchid"] != settings.mch_id
            || p["trade_state"] != "SUCCESS"
            || p["amount"]["currency"] != "CNY"
        {
            return Err(AppError::bad_request("微信支付通知无效"));
        }
        let Some(total) = p["amount"]["total"]
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .filter(|n| *n > 0)
        else {
            return Err(AppError::bad_request("微信支付通知无效"));
        };
        let field = |key: &str| {
            p[key]
                .as_str()
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| AppError::bad_request(format!("微信通知缺少 {key}")))
        };
        let paid_at = chrono::DateTime::parse_from_rfc3339(field("success_time")?)
            .map_err(|_| AppError::bad_request("微信支付时间无效"))?
            .to_utc();
        Ok(PaymentNotification {
            order_no: field("out_trade_no")?.into(),
            transaction_id: field("transaction_id")?.into(),
            amount: Decimal::new(i64::from(total), 2),
            paid_at,
        })
    }
    async fn wx_pay_http_post(
        &self,
        settings: &WxPaySettings,
        url: &str,
        body: &Value,
    ) -> Result<String, AppError> {
        // 先序列化请求体，用于构造签名消息
        let body_str = body.to_string();
        let authorization = build_wx_pay_authorization(settings, "POST", url, &body_str)?;

        let response = self
            .http
            .post(url)
            .header("Authorization", authorization)
            .header("Accept", "application/json")
            .header("User-Agent", "rust_engine/1.0")
            .header("Content-Type", "application/json; charset=utf-8")
            .body(body_str)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("微信支付请求失败: {error}")))?;

        // 收集响应头，用于后续签名验证
        let status = response.status();
        let response_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.to_string(), v.to_string()))
            })
            .collect();

        let response_body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("读取微信支付响应失败: {error}")))?;

        // 使用平台证书验证微信服务端的响应签名，防止中间人攻击
        self.verify_wx(settings, &response_headers, &response_body)
            .await?;
        if !status.is_success() {
            return Err(AppError::bad_request("微信支付下单失败"));
        }
        Ok(response_body)
    }

    async fn verify_wx(
        &self,
        settings: &WxPaySettings,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<(), AppError> {
        let serial = signature_headers(headers)?.serial;
        let key = format!("Payment:WeiXin:Certificates:{}", settings.mch_id);
        let refresh_key = format!("{key}:refresh");
        let lookup = async || -> Result<Option<Vec<PlatformCertificate>>, AppError> {
            let cached = self
                .cache
                .get_string(&key)
                .await
                .ok()
                .flatten()
                .and_then(|s| serde_json::from_str::<Vec<PlatformCertificate>>(&s).ok());
            if cached
                .as_ref()
                .is_some_and(|c| c.iter().any(|e| e.serial_no.eq_ignore_ascii_case(serial)))
            {
                return Ok(cached);
            }
            if self.cache.get_string(&refresh_key).await?.is_some() {
                return Err(AppError::bad_request("微信支付平台证书刷新过于频繁"));
            }
            Ok(None)
        };
        let certs = if let Some(certs) = lookup().await? {
            certs
        } else {
            let guard = self
                .lock
                .lock(&format!("{key}:lock"), Duration::ZERO, 1)
                .await?;
            let result: Result<_, AppError> = async {
                if let Some(certs) = lookup().await? {
                    return Ok(certs);
                }
                self.cache
                    .set_string_with_options(
                        &refresh_key,
                        "1",
                        CacheEntryOptions::relative(Duration::from_secs(60)),
                    )
                    .await?;
                let certs = self.request_wx_pay_certificates(settings).await?;
                if let Ok(json) = serde_json::to_string(&certs) {
                    let _ = self
                        .cache
                        .set_string_with_options(
                            &key,
                            &json,
                            CacheEntryOptions::relative(Duration::from_secs(
                                WX_PAY_CERT_CACHE_TTL_SECONDS,
                            )),
                        )
                        .await;
                }
                Ok(certs)
            }
            .await;
            guard.release().await;
            result?
        };
        verify_wx_pay_response_sign(headers, body, &certs).map_err(AppError::bad_request)
    }

    /// 从微信支付平台拉取证书列表并解密
    ///
    /// 证书用于验证微信响应和回调的签名，缓存未命中时才会实际请求微信接口
    async fn request_wx_pay_certificates(
        &self,
        settings: &WxPaySettings,
    ) -> Result<Vec<PlatformCertificate>, AppError> {
        let url = format!("{}/v3/certificates", WX_PAY_BASE_URL);

        // 拉取证书接口本身也需要签名鉴权，请求体为空
        let authorization = build_wx_pay_authorization(settings, "GET", &url, "")?;

        let response = self
            .http
            .get(&url)
            .header("Authorization", authorization)
            .header("Accept", "application/json")
            .header("User-Agent", "rust_engine/1.0")
            .send()
            .await
            .map_err(|error| AppError::internal(format!("获取微信支付证书请求失败: {error}")))?;

        if !response.status().is_success() {
            return Err(AppError::bad_request(format!(
                "微信支付网关返回 {}",
                response.status()
            )));
        }
        let response_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.to_string(), value.to_string()))
            })
            .collect();

        let response_body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("读取微信支付证书响应失败: {error}")))?;

        let cert_resp: WxPayCertificatesResponse = serde_json::from_str(&response_body)
            .map_err(|error| AppError::internal(format!("解析微信支付证书响应失败: {error}")))?;

        // 证书密文使用 API V3 密钥通过 AES-256-GCM 加密，保留原元数据。
        let mut certificates = Vec::with_capacity(cert_resp.data.len());
        for entry in cert_resp.data {
            let pem = decrypt_resource(&entry.encrypt_certificate, &settings.mch_api_v3_key)?;
            parse_certificate(&pem, &entry.serial_no).map_err(AppError::bad_request)?;
            certificates.push(PlatformCertificate {
                serial_no: entry.serial_no,
                certificate: pem,
            });
        }

        // 证书下载响应使用本次解密所得的平台证书验签，成功后调用方才可写入缓存。
        verify_wx_pay_response_sign(&response_headers, &response_body, &certificates).map_err(
            |error| AppError::internal(format!("微信支付证书响应签名验证失败: {error}")),
        )?;

        Ok(certificates)
    }
}
pub(super) struct PrepayOrder<'a> {
    pub number: &'a str,
    pub amount: Decimal,
    pub title: String,
    pub notify_url: String,
}
fn decimal_to_fen(price: Decimal) -> Result<i32, AppError> {
    let error = || AppError::bad_request("微信支付金额必须为有效的整数分");
    let cents = price.checked_mul(Decimal::from(100)).ok_or_else(error)?;
    if cents <= Decimal::ZERO || cents.fract() != Decimal::ZERO {
        return Err(error());
    }
    i32::try_from(cents).map_err(|_| error())
}

/// 生成用于微信支付签名的随机字符串
///
/// 使用系统随机源生成 16 字节随机数，以大写十六进制编码
fn generate_nonce() -> String {
    use rsa::rand_core::{OsRng, RngCore};

    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// 构建微信支付请求的 Authorization 鉴权头
///
/// 使用 WECHATPAY2-SHA256-RSA2048 方案，签名消息格式为：
/// HTTP方法\n请求路径\n时间戳\n随机串\n请求体\n
fn build_wx_pay_authorization(
    settings: &WxPaySettings,
    method: &str,
    url: &str,
    body: &str,
) -> Result<String, AppError> {
    let time_stamp = Utc::now().timestamp();
    let nonce_str = generate_nonce();

    // 签名消息中只需要 URL 路径部分，去掉域名前缀
    let url_path = url
        .trim_start_matches("https://api.mch.weixin.qq.com")
        .trim_start_matches("http://api.mch.weixin.qq.com");

    let message = format!(
        "{}\n{}\n{}\n{}\n{}\n",
        method, url_path, time_stamp, nonce_str, body
    );
    let signature = wx_pay_rsa_sign(&settings.mch_api_cert_key, &message)?;

    Ok(format!(
        "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",timestamp=\"{}\",serial_no=\"{}\",signature=\"{}\"",
        settings.mch_id, nonce_str, time_stamp, settings.mch_api_cert_id, signature
    ))
}

/// 使用商户 API 证书私钥对消息进行 RSA-PKCS1v15-SHA256 签名
///
/// 兼容 PKCS1（BEGIN RSA PRIVATE KEY）和 PKCS8（BEGIN PRIVATE KEY）两种 PEM 格式
fn wx_pay_rsa_sign(private_key_pem: &str, message: &str) -> Result<String, AppError> {
    use common::crypto_helper::{
        RsaHashAlgorithm, RsaSignaturePadding, StringEncoding, rsa_sign_data,
    };

    rsa_sign_data(
        private_key_pem,
        message,
        StringEncoding::Base64,
        RsaHashAlgorithm::Sha256,
        RsaSignaturePadding::Pkcs1,
    )
    .map_err(|error| AppError::internal(format!("微信支付 RSA 签名失败: {error}")))
}

fn decrypt_resource(resource: &WxPayEncryptedCert, key: &str) -> Result<String, AppError> {
    use common::crypto_helper::{StringEncoding, aes_gcm_decrypt};
    if resource.algorithm.as_deref() != Some("AEAD_AES_256_GCM")
        || resource.nonce.len() != 12
        || key.len() != 32
    {
        return Err(AppError::bad_request("微信支付加密参数无效"));
    }
    aes_gcm_decrypt(
        &resource.ciphertext,
        key,
        &resource.nonce,
        resource.associated_data.as_deref(),
        StringEncoding::Base64,
    )
    .map_err(|_| AppError::bad_request("微信支付资源解密失败"))
}

struct SignatureHeaders<'a> {
    serial: &'a str,
    timestamp: &'a str,
    nonce: &'a str,
    signature: &'a str,
}

fn signature_headers(headers: &HashMap<String, String>) -> Result<SignatureHeaders<'_>, AppError> {
    let field = |key: &str| {
        let mut items = headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(key));
        items
            .next()
            .map(|(_, value)| value.as_str())
            .filter(|s| !s.trim().is_empty() && items.next().is_none())
            .ok_or_else(|| AppError::bad_request("微信支付签名头不完整或重复"))
    };
    let timestamp = field("Wechatpay-Timestamp")?;
    if !timestamp.bytes().all(|b| b.is_ascii_digit())
        || !timestamp
            .parse::<i64>()
            .is_ok_and(|t| Utc::now().timestamp().abs_diff(t) <= 300)
    {
        return Err(AppError::bad_request("微信支付签名时间无效"));
    }
    Ok(SignatureHeaders {
        timestamp,
        serial: field("Wechatpay-Serial")?,
        nonce: field("Wechatpay-Nonce")?,
        signature: field("Wechatpay-Signature")?,
    })
}

fn parse_certificate(pem: &str, serial: &str) -> Result<x509_cert::Certificate, String> {
    use x509_cert::der::Decode;
    let bytes = pem::parse(pem).map_err(|e| e.to_string())?;
    let cert = x509_cert::Certificate::from_der(bytes.contents()).map_err(|e| e.to_string())?;
    let actual: String = cert
        .tbs_certificate()
        .serial_number()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect();
    if !actual.eq_ignore_ascii_case(serial) {
        return Err("微信支付证书序列号不匹配".into());
    }
    Ok(cert)
}

/// 验证微信支付应答或回调的报文签名
///
/// 从响应头中提取时间戳、随机串、序列号和签名，依据序列号找到对应平台证书，
/// 从证书中提取 RSA 公钥，使用 PKCS1v15-SHA256 方案验证签名合法性
///
fn verify_wx_pay_response_sign(
    headers: &HashMap<String, String>,
    body: &str,
    certificates: &[PlatformCertificate],
) -> Result<(), String> {
    use rsa::sha2::Sha256;
    use rsa::{RsaPublicKey, pkcs1v15::VerifyingKey, pkcs8::DecodePublicKey, signature::Verifier};
    use x509_cert::der::Encode;

    let SignatureHeaders {
        serial,
        timestamp,
        nonce,
        signature: signature_b64,
    } = signature_headers(headers).map_err(|e| e.message().to_string())?;

    // 根据序列号找到对应的平台证书 PEM
    let cert_pem = certificates
        .iter()
        .find(|entry| entry.serial_no.eq_ignore_ascii_case(serial))
        .map(|entry| entry.certificate.as_str())
        .ok_or_else(|| format!("未找到序列号 {} 对应的平台证书", serial))?;

    // 解析 X.509 证书，从中提取 SubjectPublicKeyInfo 并转为 RSA 公钥
    let cert = parse_certificate(cert_pem, serial)?;
    let validity = cert.tbs_certificate().validity();
    let now = std::time::SystemTime::now();
    if now < validity.not_before.to_system_time() || now > validity.not_after.to_system_time() {
        return Err("微信支付验签证书已失效".into());
    }
    let spki_der = cert
        .tbs_certificate()
        .subject_public_key_info()
        .to_der()
        .map_err(|error| format!("导出公钥 DER 失败: {error}"))?;
    let public_key = RsaPublicKey::from_public_key_der(&spki_der)
        .map_err(|error| format!("解析平台证书公钥失败: {error}"))?;

    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|error| format!("解码签名 Base64 失败: {error}"))?;
    let signature = rsa::pkcs1v15::Signature::try_from(signature_bytes.as_slice())
        .map_err(|error| format!("解析签名失败: {error}"))?;

    // 签名消息格式：时间戳\n随机串\n响应体\n
    let message = format!("{}\n{}\n{}\n", timestamp, nonce, body);
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_| "微信支付响应签名验证失败".to_string())
}
