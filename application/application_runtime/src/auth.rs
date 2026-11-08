use common::app_error::AppError;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header};
use serde::Serialize;

/// ECDSA JWT 编解码器
pub struct EcJwtCodec {
    /// JWT 编码密钥
    encoding_key: EncodingKey,

    /// JWT 验签密钥
    decoding_key: DecodingKey,
}

impl EcJwtCodec {
    /// 创建 ECDSA JWT 编解码器
    pub fn new(encoding_key: EncodingKey, decoding_key: DecodingKey) -> Self {
        Self {
            encoding_key,
            decoding_key,
        }
    }

    /// 对 JWT Claims 进行签名
    pub fn sign<T>(&self, claims: &T) -> Result<String, AppError>
    where
        T: Serialize,
    {
        jsonwebtoken::encode(&Header::new(Algorithm::ES256), claims, &self.encoding_key)
            .map_err(|error| AppError::internal(format!("Token 生成失败: {error}")))
    }

    /// 获取 JWT 验签密钥
    pub fn decoding_key(&self) -> &DecodingKey {
        &self.decoding_key
    }

    /// 提取 JWT 的签名段
    pub fn encoded_signature<'a>(&self, token: &'a str) -> Option<&'a str> {
        token.split('.').nth(2)
    }
}
