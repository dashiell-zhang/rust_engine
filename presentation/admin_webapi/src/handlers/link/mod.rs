use application_model::link::models::{
    DeleteLinkQuery, EditLinkDto, LinkDto, LinkIdQuery, LinkPageRequest,
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

/// 获取友情链接分页列表
#[utoipa::path(
    get,
    path = "/Site/GetLinkList",
    params(LinkPageRequest),
    responses(
        (status = 200, description = "查询成功", body = PageListDto<LinkDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "友情链接"
)]
pub async fn get_link_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<LinkPageRequest>,
) -> Result<Response, AppError> {
    let service = app_state.services().link_webapi();

    match service.get_link_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取单个友情链接
#[utoipa::path(
    get,
    path = "/Site/GetLink",
    params(LinkIdQuery),
    responses(
        (status = 200, description = "查询成功", body = Option<LinkDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "友情链接"
)]
pub async fn get_link(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<LinkIdQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().link_webapi();

    match service.get_link(query.link_id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建友情链接，返回新链接 ID
#[utoipa::path(
    post,
    path = "/Site/CreateLink",
    request_body = EditLinkDto,
    responses(
        (status = 200, description = "创建成功，返回新链接 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "友情链接"
)]
pub async fn create_link(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<EditLinkDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().link_webapi();
    let current_user = user_ctx.current_user();

    match service.create_link(dto, &current_user).await {
        Ok(id) => Ok((StatusCode::OK, Json(id)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新友情链接信息
#[utoipa::path(
    post,
    path = "/Site/UpdateLink",
    params(LinkIdQuery),
    request_body = EditLinkDto,
    responses(
        (status = 200, description = "更新成功", body = bool),
        (status = 400, description = "请求参数无效或链接不存在"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "友情链接"
)]
pub async fn update_link(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<LinkIdQuery>,
    Json(dto): Json<EditLinkDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().link_webapi();
    let current_user = user_ctx.current_user();

    match service.update_link(query.link_id, dto, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 软删除友情链接
#[utoipa::path(
    delete,
    path = "/Site/DeleteLink",
    params(DeleteLinkQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "友情链接"
)]
pub async fn delete_link(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<DeleteLinkQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().link_webapi();
    let current_user = user_ctx.current_user();

    match service.delete_link(query.id, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
