use application_model::user::models::{EditUserPhoneBySmsDto, UserDto, UserIdQuery};
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

/// 获取单个用户信息
///
/// 传 user_id 则查指定用户 不传则查当前登录用户
#[utoipa::path(
    get,
    path = "/User/GetUser",
    params(UserIdQuery),
    responses(
        (status = 200, description = "查询成功", body = Option<UserDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn get_user(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(params): Query<UserIdQuery>,
) -> Result<Response, AppError> {
    // 默认回落到当前登录用户 兼容个人中心场景
    let target_id = params.user_id.unwrap_or(user_ctx.user_id);
    let service = app_state.services().user_webapi();

    match service.get_user(target_id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 通过短信验证码修改当前用户手机号
#[utoipa::path(
    post,
    path = "/User/EditUserPhoneBySms",
    request_body = EditUserPhoneBySmsDto,
    responses(
        (status = 200, description = "修改成功", body = bool),
        (status = 400, description = "参数、验证码或账户无效，或手机号已被占用"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn edit_user_phone_by_sms(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(request): Json<EditUserPhoneBySmsDto>,
) -> Result<Response, AppError> {
    // 当前用户信息用于限定只能修改自己的手机号
    let service = app_state.services().user_webapi();
    let current_user = user_ctx.current_user();

    match service.edit_user_phone_by_sms(request, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
