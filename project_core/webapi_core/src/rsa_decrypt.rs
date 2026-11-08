//! 按需启用的 RSA 请求字段解密；普通 Json/Query 不受影响。
use crate::{
    error::AppError,
    extract::{Json, Query},
    host::state::AppState,
};
use axum::{
    extract::{FromRequest, FromRequestParts, Request},
    http::request::Parts,
    response::{IntoResponse, Response},
};
use common::crypto_helper::{RsaEncryptionPadding, StringEncoding, rsa_decrypt};
use serde::Deserialize;
use source_generator_runtime::RsaDecrypt;
use std::sync::Arc;

/// 与源 RSA 配置对应。未挂载解密提取器时不要求存在该配置。
#[derive(Clone, Default, Deserialize)]
pub struct RsaSettings {
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub private_key: Option<String>,
}

/// 由宿主提供私钥；不在请求 DTO 或应用服务中保存密钥。
pub trait RsaDecryptionState {
    fn rsa_private_key(&self) -> Option<&str>;
}
impl RsaDecryptionState for AppState {
    fn rsa_private_key(&self) -> Option<&str> {
        self.settings
            .rsa
            .as_ref()
            .and_then(|settings| settings.private_key.as_deref())
    }
}
impl<T: RsaDecryptionState + ?Sized> RsaDecryptionState for Arc<T> {
    fn rsa_private_key(&self) -> Option<&str> {
        (**self).rsa_private_key()
    }
}

/// 在请求绑定后、业务调用前解密模型；也供自定义 Form 提取器使用。
pub fn decrypt_request<T: RsaDecrypt>(
    value: &mut T,
    private_key: Option<&str>,
) -> Result<(), AppError> {
    let private_key = private_key
        .ok_or_else(|| AppError::from(common::app_error::AppError::internal("私钥获取异常")))?;
    value
        .decrypt_fields(&mut |field| {
            match rsa_decrypt(
                private_key,
                field,
                StringEncoding::Base64,
                RsaEncryptionPadding::OaepSha256,
            ) {
                Ok(plaintext) => {
                    *field = plaintext;
                    Ok(())
                }
                Err(_) if cfg!(debug_assertions) => {
                    // 不记录密钥、密文、明文或底层错误文本。
                    tracing::error!(target: "webapi.rsa_decrypt", "无效的 密文");
                    Ok(())
                }
                Err(_) => Err("无效的 密文".into()),
            }
        })
        .map_err(|error| common::app_error::AppError::internal(error).into())
}

/// 对 JSON 请求模型按 rsa_encrypted 标记解密。
pub struct RsaJson<T>(pub T);
impl<S, T> FromRequest<S> for RsaJson<T>
where
    S: RsaDecryptionState + Send + Sync,
    T: common::json_helper::input::JsonModel + RsaDecrypt + Send,
{
    type Rejection = Response;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(mut value) = Json::<T>::from_request(request, state).await?;
        decrypt_request(&mut value, state.rsa_private_key())
            .map_err(IntoResponse::into_response)?;
        Ok(Self(value))
    }
}

/// 对查询绑定模型按 rsa_encrypted 标记解密。
pub struct RsaQuery<T>(pub T);
impl<S, T> FromRequestParts<S> for RsaQuery<T>
where
    S: RsaDecryptionState + Send + Sync,
    T: common::json_helper::input::QueryModel + RsaDecrypt + Send,
{
    type Rejection = Response;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(mut value) = Query::<T>::from_request_parts(parts, state).await?;
        decrypt_request(&mut value, state.rsa_private_key())
            .map_err(IntoResponse::into_response)?;
        Ok(Self(value))
    }
}
