use application_model::llm::models::{
    EditLlmAppDto, LlmAppIdQuery, LlmAppPageListDto, LlmAppPageRequest, LlmConversationPageListDto,
    LlmConversationPageRequest, TestLlmAppRequest, TestLlmAppResult,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use webapi_core::extract::{Json, PageQuery, Query};
use webapi_core::{auth::UserContext, error::AppError};

use crate::llm_support::AdminLlmApplicationAdapter;
use crate::state::AppState;

/// 获取 LLM 应用配置分页列表
#[utoipa::path(
    get,
    path = "/Operations/GetLlmAppList",
    params(LlmAppPageRequest),
    responses(
        (status = 200, description = "查询成功", body = LlmAppPageListDto),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "LLM 管理"
)]
pub async fn get_llm_app_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<LlmAppPageRequest>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app();

    match service.get_llm_app_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建 LLM 应用配置并返回配置 ID
#[utoipa::path(
    post,
    path = "/Operations/CreateLlmApp",
    request_body = EditLlmAppDto,
    responses(
        (status = 200, description = "创建成功，返回新配置 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "LLM 管理"
)]
pub async fn create_llm_app(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<EditLlmAppDto>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app();
    let current_user = user_ctx.current_user();

    match service.create_llm_app(dto, &current_user).await {
        Ok(llm_app_id) => Ok((StatusCode::OK, llm_app_id).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新指定的 LLM 应用配置
#[utoipa::path(
    post,
    path = "/Operations/UpdateLlmApp",
    params(LlmAppIdQuery),
    request_body = EditLlmAppDto,
    responses(
        (status = 200, description = "更新成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "LLM 管理"
)]
pub async fn update_llm_app(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<LlmAppIdQuery>,
    Json(dto): Json<EditLlmAppDto>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app();
    let current_user = user_ctx.current_user();

    match service
        .update_llm_app(query.llm_app_id, dto, &current_user)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 软删除指定的 LLM 应用配置
#[utoipa::path(
    delete,
    path = "/Operations/DeleteLlmApp",
    params(LlmAppIdQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "LLM 管理"
)]
pub async fn delete_llm_app(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<LlmAppIdQuery>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app();
    let current_user = user_ctx.current_user();

    match service
        .delete_llm_app(query.llm_app_id, &current_user)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 测试一次 LLM 应用配置
#[utoipa::path(
    post,
    path = "/Operations/TestLlmApp",
    request_body = TestLlmAppRequest,
    responses(
        (status = 200, description = "测试成功", body = TestLlmAppResult),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "LLM 管理"
)]
pub async fn test_llm_app(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Json(request): Json<TestLlmAppRequest>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app();

    match service.test_llm_app(request).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取 LLM 对话记录分页列表
#[utoipa::path(
    get,
    path = "/Operations/GetLlmConversationList",
    params(LlmConversationPageRequest),
    responses(
        (status = 200, description = "查询成功", body = LlmConversationPageListDto),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "LLM 管理"
)]
pub async fn get_llm_conversation_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<LlmConversationPageRequest>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_conversation_manage();

    match service.get_llm_conversation_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
