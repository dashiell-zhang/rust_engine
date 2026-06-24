use application_model::authorize::models::{
    EcdsaKeyPairDto, GetFunctionListRequest, GetTokenRequest,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose};
use common::app_error::AppError as ApplicationError;
use p256::{SecretKey, pkcs8::EncodePublicKey};
use rand_core::OsRng;
use std::sync::Arc;
use webapi_core::extract::{Json, Query};
use webapi_core::{auth::UserContext, error::AppError};

use crate::state::AppState;

/// 用户名密码登录，成功返回 JWT Token
#[utoipa::path(
    post,
    path = "/Authorize/GetToken",
    request_body = GetTokenRequest,
    responses(
        (status = 200, description = "登录成功，返回 JWT Token", body = String),
        (status = 400, description = "用户名或密码为空"),
        (status = 401, description = "用户名或密码错误"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "授权"
)]
pub async fn get_token(
    State(app_state): State<Arc<AppState>>,
    Json(request): Json<GetTokenRequest>,
) -> Result<Response, AppError> {
    let service = app_state.authorize();

    match service.get_token(request).await {
        Ok(token) => Ok(token.into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取当前用户可访问的功能列表
#[utoipa::path(
    get,
    path = "/Authorize/GetFunctionList",
    params(GetFunctionListRequest),
    responses(
        (status = 200, description = "获取成功", body = Object),
        (status = 401, description = "未授权或无权限"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "授权"
)]
pub async fn get_function_list(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(request): Query<GetFunctionListRequest>,
) -> Result<Response, AppError> {
    let service = app_state.authorize();

    match service
        .get_function_list(user_ctx.current_user(), request.sign.as_deref())
        .await
    {
        Ok(functions) => Ok(Json(functions).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新路由信息表
#[utoipa::path(
    get,
    path = "/Authorize/UpdateRoute",
    responses(
        (status = 200, description = "同步成功", body = bool),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "授权"
)]
pub async fn update_route(State(app_state): State<Arc<AppState>>) -> Result<Response, AppError> {
    match app_state.sync_permission_routes().await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 生成 ECDSA 公私钥
#[utoipa::path(
    get,
    path = "/Authorize/GenerateEcdsaKeyPair",
    responses(
        (status = 200, description = "生成成功", body = EcdsaKeyPairDto),
        (status = 400, description = "非 Development 环境不允许调用"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "授权"
)]
pub async fn generate_ecdsa_key_pair() -> Result<Response, AppError> {
    if !is_development_environment() {
        return Err(ApplicationError::bad_request(
            "GenerateEcdsaKeyPair 仅允许在 Development 环境执行",
        )
        .into());
    }

    let secret_key = SecretKey::random(&mut OsRng);
    let private_key = secret_key.to_sec1_der().map_err(|error| {
        AppError::from(ApplicationError::internal(format!(
            "ECDSA 私钥编码失败: {error}"
        )))
    })?;
    let public_key = secret_key
        .public_key()
        .to_public_key_der()
        .map_err(|error| {
            AppError::from(ApplicationError::internal(format!(
                "ECDSA 公钥编码失败: {error}"
            )))
        })?;

    let result = EcdsaKeyPairDto {
        private_key: general_purpose::STANDARD.encode(private_key.as_slice()),
        public_key: general_purpose::STANDARD.encode(public_key.as_bytes()),
    };

    Ok((StatusCode::OK, Json(result)).into_response())
}

fn is_development_environment() -> bool {
    std::env::var("APP_ENVIRONMENT")
        .or_else(|_| std::env::var("ASPNETCORE_ENVIRONMENT"))
        .map(|value| value.eq_ignore_ascii_case("Development"))
        .unwrap_or(false)
}
