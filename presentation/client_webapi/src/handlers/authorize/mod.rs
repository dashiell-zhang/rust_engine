use application_model::authorize::models::{
    ClientGetFunctionListRequest, GetTokenBySmsRequest, GetTokenByWxAppRequest,
    GetTokenByWxMiniAppRequest, GetTokenRequest, SendSmsVerifyCodeRequest,
    UpdatePasswordByOldPasswordRequest, UpdatePasswordBySmsRequest,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use webapi_core::extract::{Json, Query};
use webapi_core::{auth::UserContext, error::AppError};
use webapi_support::WebApiApplicationExt;

use crate::state::AppState;

/// 显式同步当前客户端宿主需要鉴权的路由。
#[utoipa::path(
    get,
    path = "/Authorize/UpdateRoute",
    responses(
        (status = 200, description = "同步成功，空正文"),
        (status = 500, description = "同步失败"),
    ),
    tag = "授权"
)]
pub async fn update_route(State(app_state): State<Arc<AppState>>) -> Result<Response, AppError> {
    app_state.sync_permission_routes().await?;
    Ok(StatusCode::OK.into_response())
}

/// 获取配置中的 RSA 公钥。
#[utoipa::path(
    get,
    path = "/Authorize/GetPublicKey",
    responses(
        (status = 200, description = "RSA 公钥", body = String, content_type = "text/plain"),
        (status = 204, description = "已配置 RSA，但没有公钥"),
        (status = 400, description = "RSA 配置不存在"),
    ),
    tag = "授权"
)]
pub async fn get_public_key(State(app_state): State<Arc<AppState>>) -> Result<Response, AppError> {
    match app_state.services().authorize().get_public_key()? {
        Some(key) => Ok(webapi_core::text_response::Text(key).into_response()),
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
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
    // Handler 只负责接收请求和返回协议层响应
    let service = app_state.services().authorize();

    match service.get_token(request).await {
        Ok(token) => Ok(webapi_core::text_response::Text(token).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 手机号短信验证码登录，成功返回 JWT Token
#[utoipa::path(
    post,
    path = "/Authorize/GetTokenBySMS",
    request_body = GetTokenBySmsRequest,
    responses(
        (status = 200, description = "登录成功，返回 JWT Token", body = String),
        (status = 400, description = "手机号或验证码无效"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "授权"
)]
pub async fn get_token_by_sms(
    State(app_state): State<Arc<AppState>>,
    Json(request): Json<GetTokenBySmsRequest>,
) -> Result<Response, AppError> {
    // 短信登录和账号密码登录共用同一个授权服务
    let service = app_state.services().authorize();

    match service.get_token_by_sms(request).await {
        Ok(token) => Ok(webapi_core::text_response::Text(token).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取当前用户可访问的功能列表
#[utoipa::path(
    get,
    path = "/Authorize/GetFunctionList",
    params(ClientGetFunctionListRequest),
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
    Query(request): Query<ClientGetFunctionListRequest>,
) -> Result<Response, AppError> {
    // 当前用户从鉴权中间件注入 这里仅透传给服务层
    let service = app_state.services().authorize();

    match service
        .get_function_list(user_ctx.current_user(), Some(&request.sign))
        .await
    {
        Ok(functions) => Ok(Json(functions).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 发送短信验证码
#[utoipa::path(
    post,
    path = "/Authorize/SendSMSVerifyCode",
    request_body = SendSmsVerifyCodeRequest,
    responses(
        (status = 200, description = "发送成功", body = bool),
        (status = 400, description = "请求参数无效或发送过于频繁"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "授权"
)]
pub async fn send_sms_verify_code(
    State(app_state): State<Arc<AppState>>,
    Json(request): Json<SendSmsVerifyCodeRequest>,
) -> Result<Response, AppError> {
    // 发送接口只返回提交结果 实际短信由后台任务异步处理
    let service = app_state.services().authorize();

    match service.send_sms_verify_code(request).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 微信小程序登录，成功返回 JWT Token
#[utoipa::path(
    post,
    path = "/Authorize/GetTokenByWeiXinMiniApp",
    request_body = GetTokenByWxMiniAppRequest,
    responses(
        (status = 200, description = "登录成功，返回 JWT Token", body = String),
        (status = 400, description = "请求格式错误，或非开发环境的内部错误"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "授权"
)]
pub async fn get_token_by_wx_mini_app(
    State(app_state): State<Arc<AppState>>,
    Json(request): Json<GetTokenByWxMiniAppRequest>,
) -> Result<Response, AppError> {
    // 小程序登录复用授权服务中的第三方绑定逻辑
    let service = app_state.services().authorize();

    match service.get_token_by_wx_mini_app(request).await {
        Ok(token) => Ok(webapi_core::text_response::Text(token).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 微信公众号网页登录，成功返回 JWT Token
#[utoipa::path(
    post,
    path = "/Authorize/GetTokenByWeiXinApp",
    request_body = GetTokenByWxAppRequest,
    responses(
        (status = 200, description = "登录成功，返回 JWT Token", body = String),
        (status = 400, description = "请求格式错误、微信授权失败，或非开发环境的内部错误"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "授权"
)]
pub async fn get_token_by_wx_app(
    State(app_state): State<Arc<AppState>>,
    Json(request): Json<GetTokenByWxAppRequest>,
) -> Result<Response, AppError> {
    let service = app_state.services().authorize();

    match service.get_token_by_wx_app(request).await {
        Ok(token) => Ok(webapi_core::text_response::Text(token).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 通过短信验证码修改密码
#[utoipa::path(
    post,
    path = "/Authorize/UpdatePasswordBySMS",
    request_body = UpdatePasswordBySmsRequest,
    responses(
        (status = 200, description = "修改成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "授权"
)]
pub async fn update_password_by_sms(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(request): Json<UpdatePasswordBySmsRequest>,
) -> Result<Response, AppError> {
    // 修改密码前必须先通过鉴权获取当前用户身份
    let service = app_state.services().authorize();

    match service
        .update_password_by_sms(user_ctx.current_user(), request)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 通过旧密码修改密码
#[utoipa::path(
    post,
    path = "/Authorize/UpdatePasswordByOldPassword",
    request_body = UpdatePasswordByOldPasswordRequest,
    responses(
        (status = 200, description = "修改成功", body = bool),
        (status = 400, description = "请求参数无效或原始密码错误"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "授权"
)]
pub async fn update_password_by_old_password(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(request): Json<UpdatePasswordByOldPasswordRequest>,
) -> Result<Response, AppError> {
    let service = app_state.services().authorize();

    match service
        .update_password_by_old_password(user_ctx.current_user(), request)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
