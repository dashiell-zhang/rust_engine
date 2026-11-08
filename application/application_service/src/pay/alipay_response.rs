use std::{collections::HashMap, fmt};

use application_interface::error::AppError;
use common::crypto_helper::{
    RsaHashAlgorithm, RsaSignaturePadding, StringEncoding, rsa_verify_data,
};
use serde::de::{self, MapAccess, Visitor};
use serde_json::{Value, value::RawValue};

/// 保留响应节点原文验签，不能把解析后的 JSON 再序列化作为签名正文。
pub(super) fn parse_response(
    body: &str,
    method: &str,
    public_key: &str,
) -> Result<Value, AppError> {
    let mut deserializer = serde_json::Deserializer::from_str(body);
    let fields = serde::Deserializer::deserialize_map(&mut deserializer, ResponseFields)
        .map_err(|_| AppError::internal("支付宝接口响应格式无效"))?;
    deserializer
        .end()
        .map_err(|_| AppError::internal("支付宝接口响应格式无效"))?;

    // 与源 SDK 的公钥模式一致，空公钥不执行响应验签。
    if !public_key.is_empty() {
        let root = format!("{}_response", method.replace('.', "_"));
        let raw = fields
            .get(root.as_str())
            .or_else(|| fields.get("error_response"))
            .ok_or_else(|| AppError::internal("支付宝响应缺少业务节点"))?;
        let payload: Value = serde_json::from_str(raw.get())
            .map_err(|_| AppError::internal("支付宝业务响应格式无效"))?;
        let signature = fields
            .get("sign")
            .map(|value| serde_json::from_str::<Option<String>>(value.get()))
            .transpose()
            .map_err(|_| AppError::internal("支付宝响应签名格式无效"))?
            .flatten()
            .unwrap_or_default();
        // SDK 的 IsError 由非空 sub_code 决定，而不是 code 是否为 10000。
        let is_error = payload.get("sub_code").is_some_and(|value| match value {
            Value::Null => false,
            Value::String(value) => !value.is_empty(),
            _ => true,
        });
        if !is_error || !signature.is_empty() {
            if signature.is_empty() {
                return Err(AppError::internal("支付宝响应缺少签名"));
            }
            let verify = |content: &str| {
                rsa_verify_data(
                    public_key,
                    content,
                    &signature,
                    StringEncoding::Base64,
                    RsaHashAlgorithm::Sha256,
                    RsaSignaturePadding::Pkcs1,
                )
                .map_err(|_| AppError::internal("支付宝响应签名或公钥格式无效"))
            };
            let content = raw.get();
            let valid = verify(content)?
                || (content.contains("\\/") && verify(&content.replace("\\/", "/"))?);
            if !valid {
                return Err(AppError::internal("支付宝响应签名校验失败"));
            }
        }
    }

    serde_json::from_str(body).map_err(|_| AppError::internal("支付宝接口响应格式无效"))
}

/// 拒绝重复根字段，避免验签节点与业务读取节点不一致。
struct ResponseFields;

impl<'de> Visitor<'de> for ResponseFields {
    type Value = HashMap<String, &'de RawValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("支付宝响应对象")
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut fields = HashMap::new();
        while let Some((key, value)) = map.next_entry::<String, &'de RawValue>()? {
            if fields.insert(key, value).is_some() {
                return Err(de::Error::custom("支付宝响应包含重复根字段"));
            }
        }
        Ok(fields)
    }
}
