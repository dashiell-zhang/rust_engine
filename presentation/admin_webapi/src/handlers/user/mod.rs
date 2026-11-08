use application_model::user::models::{
    DeleteUserQuery, EditUserDto, PageListDto, SetUserFunctionDto, SetUserRoleDto, UpdateUserQuery,
    UserDto, UserFunctionDto, UserIdQuery, UserPageRequest, UserRoleDto,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use webapi_core::extract::{Json, PageQuery, Query};
use webapi_core::{auth::UserContext, error::AppError};
use webapi_support::WebApiApplicationExt;

use crate::state::AppState;

/// 获取用户列表（分页，支持关键词搜索）
#[utoipa::path(
    get,
    path = "/User/GetUserList",
    params(UserPageRequest),
    responses(
        (status = 200, description = "查询成功", body = PageListDto<UserDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn get_user_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<UserPageRequest>,
) -> Result<Response, AppError> {
    let service = app_state.services().user_webapi();

    match service.get_user_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取单个用户信息，传 user_id 则查指定用户，不传则查当前登录用户
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
    let target_id = params.user_id.unwrap_or(user_ctx.user_id);
    let service = app_state.services().user_webapi();

    match service.get_user(target_id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取用户角色分配列表
#[utoipa::path(
    get,
    path = "/User/GetUserRoleList",
    params(UpdateUserQuery),
    responses(
        (status = 200, description = "查询成功", body = Vec<UserRoleDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn get_user_role_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<UpdateUserQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().user_webapi();

    match service.get_user_role_list(query.user_id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 设置用户角色分配
#[utoipa::path(
    post,
    path = "/User/SetUserRole",
    request_body = SetUserRoleDto,
    responses(
        (status = 200, description = "设置成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn set_user_role(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<SetUserRoleDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().user_webapi();
    let current_user = user_ctx.current_user();

    match service.set_user_role(dto, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取用户功能分配树
#[utoipa::path(
    get,
    path = "/User/GetUserFunction",
    params(UpdateUserQuery),
    responses(
        (status = 200, description = "查询成功", body = Vec<UserFunctionDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn get_user_function(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<UpdateUserQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().user_webapi();

    match service.get_user_function(query.user_id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 设置用户功能分配
#[utoipa::path(
    post,
    path = "/User/SetUserFunction",
    request_body = SetUserFunctionDto,
    responses(
        (status = 200, description = "设置成功", body = bool),
        (status = 400, description = "请求参数无效或权限继承自角色"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn set_user_function(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<SetUserFunctionDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().user_webapi();
    let current_user = user_ctx.current_user();

    match service.set_user_function(dto, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建用户，返回新用户 ID
#[utoipa::path(
    post,
    path = "/User/CreateUser",
    request_body = EditUserDto,
    responses(
        (status = 200, description = "创建成功，返回新用户 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn create_user(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<EditUserDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().user_webapi();
    let current_user = user_ctx.current_user();

    match service.create_user(dto, &current_user).await {
        Ok(user_id) => Ok((StatusCode::OK, Json(user_id)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新用户信息
#[utoipa::path(
    post,
    path = "/User/UpdateUser",
    params(UpdateUserQuery),
    request_body = EditUserDto,
    responses(
        (status = 200, description = "更新成功", body = bool),
        (status = 400, description = "请求参数无效或用户不存在"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn update_user(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<UpdateUserQuery>,
    Json(dto): Json<EditUserDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().user_webapi();
    let current_user = user_ctx.current_user();

    match service.update_user(query.user_id, dto, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 软删除用户
#[utoipa::path(
    delete,
    path = "/User/DeleteUser",
    params(DeleteUserQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "用户管理"
)]
pub async fn delete_user(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<DeleteUserQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().user_webapi();
    let current_user = user_ctx.current_user();

    match service.delete_user(query.id, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
