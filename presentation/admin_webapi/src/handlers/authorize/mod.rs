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
use p256::{SecretKey, elliptic_curve::Generate, pkcs8::EncodePublicKey};
use std::sync::Arc;
use webapi_core::environment::is_development as is_development_environment;
use webapi_core::extract::{Json, Query};
use webapi_core::{auth::UserContext, error::AppError};
use webapi_support::WebApiApplicationExt;

use crate::state::AppState;

/// 从可执行文件旁的初始化文件创建或恢复权限基础数据。
#[utoipa::path(
    post,
    path = "/Authorize/InitData",
    responses(
        (status = 200, description = "初始化完成", body = application_model::authorize::seed::AuthorizeInitDataResult),
        (status = 400, description = "仅允许 Development 环境调用"),
        (status = 500, description = "文件读取或初始化失败"),
    ),
    tag = "授权"
)]
pub async fn init_data(State(app_state): State<Arc<AppState>>) -> Result<Response, AppError> {
    use application_model::authorize::seed::{AuthorizeInitDataResult, AuthorizeSeedData};

    if !is_development_environment() {
        return Err(ApplicationError::bad_request("InitData 仅允许在 Development 环境执行").into());
    }
    let executable = std::env::current_exe()
        .map_err(|error| ApplicationError::internal(format!("无法定位管理端程序: {error}")))?;
    let directory = executable
        .parent()
        .ok_or_else(|| ApplicationError::internal("无法定位管理端程序目录"))?;
    let file_path = directory.join("InitData").join("AuthorizeInitData.json");
    let json = tokio::fs::read_to_string(&file_path)
        .await
        .map_err(|error| ApplicationError::internal(format!("读取初始化数据文件失败: {error}")))?;
    let seed = AuthorizeSeedData::from_json(&json)
        .map_err(|error| ApplicationError::internal(format!("初始化数据文件解析失败: {error}")))?;
    let result = app_state.services().authorize()?.init_data(seed).await?;
    Ok(Json(AuthorizeInitDataResult {
        result,
        file_path: file_path.to_string_lossy().into_owned(),
    })
    .into_response())
}

/// 用户名密码登录，成功返回 JWT Token
#[utoipa::path(
    post,
    path = "/Authorize/GetToken",
    request_body = GetTokenRequest,
    responses(
        (status = 200, description = "登录成功，返回 JWT Token", body = String),
        (status = 400, description = "用户名或密码为空或不匹配"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "授权"
)]
pub async fn get_token(
    State(app_state): State<Arc<AppState>>,
    Json(request): Json<GetTokenRequest>,
) -> Result<Response, AppError> {
    let service = app_state.services().authorize()?;

    match service.get_token(request).await {
        Ok(token) => Ok(webapi_core::text_response::Text(token).into_response()),
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
    let service = app_state.services().authorize()?;

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
        (status = 200, description = "同步成功，空正文"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "授权"
)]
pub async fn update_route(State(app_state): State<Arc<AppState>>) -> Result<Response, AppError> {
    match app_state.sync_permission_routes().await {
        Ok(_) => Ok(StatusCode::OK.into_response()),
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

    let secret_key = SecretKey::generate();
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
