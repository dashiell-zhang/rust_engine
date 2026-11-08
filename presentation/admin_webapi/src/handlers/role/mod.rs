use application_model::role::models::{
    DeleteRoleQuery, EditRoleDto, RoleDto, RoleFunctionDto, RoleIdQuery, RolePageRequest,
    RoleSelectDto, SetRoleFunctionDto,
};
use application_model::shared::models::PageListDto;
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

/// 获取角色列表
#[utoipa::path(
    get,
    path = "/Role/GetRoleList",
    params(RolePageRequest),
    responses(
        (status = 200, description = "查询成功", body = PageListDto<RoleDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "角色管理"
)]
pub async fn get_role_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<RolePageRequest>,
) -> Result<Response, AppError> {
    let service = app_state.services().role();

    match service.get_role_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取单个角色信息
#[utoipa::path(
    get,
    path = "/Role/GetRole",
    params(RoleIdQuery),
    responses(
        (status = 200, description = "查询成功", body = Option<RoleDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "角色管理"
)]
pub async fn get_role(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<RoleIdQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().role();

    match service.get_role(query.role_id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建角色
#[utoipa::path(
    post,
    path = "/Role/CreateRole",
    request_body = EditRoleDto,
    responses(
        (status = 200, description = "创建成功，返回新角色 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "角色管理"
)]
pub async fn create_role(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Json(dto): Json<EditRoleDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().role();

    match service.create_role(dto).await {
        Ok(role_id) => Ok((StatusCode::OK, Json(role_id)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新角色
#[utoipa::path(
    post,
    path = "/Role/UpdateRole",
    params(RoleIdQuery),
    request_body = EditRoleDto,
    responses(
        (status = 200, description = "更新成功", body = bool),
        (status = 400, description = "请求参数无效或角色不存在"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "角色管理"
)]
pub async fn update_role(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<RoleIdQuery>,
    Json(dto): Json<EditRoleDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().role();

    match service.update_role(query.role_id, dto).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 删除角色
#[utoipa::path(
    delete,
    path = "/Role/DeleteRole",
    params(DeleteRoleQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 400, description = "当前角色下存在人员信息"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "角色管理"
)]
pub async fn delete_role(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<DeleteRoleQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().role();

    match service.delete_role(query.id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取角色功能树
#[utoipa::path(
    get,
    path = "/Role/GetRoleFunction",
    params(RoleIdQuery),
    responses(
        (status = 200, description = "查询成功", body = Vec<RoleFunctionDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "角色管理"
)]
pub async fn get_role_function(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<RoleIdQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().role();

    match service.get_role_function(query.role_id).await {
        Ok(list) => Ok((StatusCode::OK, Json(list)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 设置角色功能勾选状态
#[utoipa::path(
    post,
    path = "/Role/SetRoleFunction",
    request_body = SetRoleFunctionDto,
    responses(
        (status = 200, description = "设置成功", body = bool),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "角色管理"
)]
pub async fn set_role_function(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<SetRoleFunctionDto>,
) -> Response {
    let service = app_state.services().role();
    let current_user = user_ctx.current_user();

    match service.set_role_function(dto, &current_user).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => AppError::from(error).into_response(),
    }
}

/// 获取角色下拉列表
#[utoipa::path(
    get,
    path = "/Role/GetRoleSelect",
    responses(
        (status = 200, description = "查询成功", body = Vec<RoleSelectDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "角色管理"
)]
pub async fn get_role_select(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
) -> Result<Response, AppError> {
    let service = app_state.services().role();

    match service.get_role_select().await {
        Ok(list) => Ok((StatusCode::OK, Json(list)).into_response()),
        Err(error) => Err(error.into()),
    }
}
