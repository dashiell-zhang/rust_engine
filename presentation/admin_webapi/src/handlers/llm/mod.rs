use application_model::llm::models::{
    EditLlmAppDto, EditLlmModelDto, LlmAppDto, LlmAppIdQuery, LlmAppPageRequest,
    LlmConversationDto, LlmConversationPageRequest, LlmModelDto, LlmModelIdQuery,
    LlmModelPageRequest, LlmModelSelectDto, TestLlmAppRequest, TestLlmAppResult,
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

use crate::llm_support::AdminLlmApplicationAdapter;
use crate::state::AppState;

/// 获取 LLM 模型配置分页列表
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "获取 LLM 模型配置列表",
    get,
    path = "/LlmModel/GetLlmModelList",
    params(LlmModelPageRequest),
    responses(
        (status = 200, description = "查询成功", body = PageListDto<LlmModelDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn get_llm_model_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<LlmModelPageRequest>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_model()?;

    match service.get_llm_model_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取启用的 LLM 模型下拉列表
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "获取 LLM 模型下拉列表",
    get,
    path = "/LlmModel/GetLlmModelSelect",
    responses(
        (status = 200, description = "查询成功", body = Vec<LlmModelSelectDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn get_llm_model_select(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_model()?;

    match service.get_llm_model_select().await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建 LLM 模型配置并返回配置 ID
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "创建 LLM 模型配置",
    json_body = "createLlmModel",
    post,
    path = "/LlmModel/CreateLlmModel",
    request_body = EditLlmModelDto,
    responses(
        (status = 200, description = "创建成功，返回新配置 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn create_llm_model(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<EditLlmModelDto>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_model()?;
    let current_user = user_ctx.current_user();

    match service.create_llm_model(dto, &current_user).await {
        Ok(id) => Ok((StatusCode::OK, Json(id)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新指定的 LLM 模型配置
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "更新 LLM 模型配置",
    json_body = "updateLlmModel",
    post,
    path = "/LlmModel/UpdateLlmModel",
    params(LlmModelIdQuery),
    request_body = EditLlmModelDto,
    responses(
        (status = 200, description = "更新成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn update_llm_model(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<LlmModelIdQuery>,
    Json(dto): Json<EditLlmModelDto>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_model()?;
    let current_user = user_ctx.current_user();

    match service.update_llm_model(query.id, dto, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 软删除指定的 LLM 模型配置
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "删除 LLM 模型配置",
    delete,
    path = "/LlmModel/DeleteLlmModel",
    params(LlmModelIdQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn delete_llm_model(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<LlmModelIdQuery>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_model()?;
    let current_user = user_ctx.current_user();

    match service.delete_llm_model(query.id, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取 LLM 应用配置分页列表
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "获取 LLM 应用配置列表",
    get,
    path = "/Operations/GetLlmAppList",
    params(LlmAppPageRequest),
    responses(
        (status = 200, description = "查询成功", body = PageListDto<LlmAppDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn get_llm_app_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<LlmAppPageRequest>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app()?;

    match service.get_llm_app_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建 LLM 应用配置并返回配置 ID
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "创建 LLM 应用配置",
    json_body = "createLlmApp",
    post,
    path = "/Operations/CreateLlmApp",
    request_body = EditLlmAppDto,
    responses(
        (status = 200, description = "创建成功，返回新配置 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn create_llm_app(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<EditLlmAppDto>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app()?;
    let current_user = user_ctx.current_user();

    match service.create_llm_app(dto, &current_user).await {
        Ok(llm_app_id) => Ok((StatusCode::OK, Json(llm_app_id)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新指定的 LLM 应用配置
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "更新 LLM 应用配置",
    json_body = "updateLlmApp",
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
    tag = "LLM 管理"
)]
pub async fn update_llm_app(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<LlmAppIdQuery>,
    Json(dto): Json<EditLlmAppDto>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app()?;
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
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "删除 LLM 应用配置",
    delete,
    path = "/Operations/DeleteLlmApp",
    params(LlmAppIdQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn delete_llm_app(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<LlmAppIdQuery>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app()?;
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
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "LLM 调用测试",
    json_body = "request",
    post,
    path = "/Operations/TestLlmApp",
    request_body = TestLlmAppRequest,
    responses(
        (status = 200, description = "测试成功", body = TestLlmAppResult),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn test_llm_app(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    cancellation: webapi_core::request_cancellation::RequestCancellation,
    Json(request): Json<TestLlmAppRequest>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_app()?;

    match service
        .test_llm_app_with_cancellation(user_ctx.current_user().user_id, request, cancellation.0)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取 LLM 对话记录分页列表
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "获取 LLM 调用日志列表",
    get,
    path = "/Operations/GetLlmConversationList",
    params(LlmConversationPageRequest),
    responses(
        (status = 200, description = "查询成功", body = PageListDto<LlmConversationDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "LLM 管理"
)]
pub async fn get_llm_conversation_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<LlmConversationPageRequest>,
) -> Result<Response, AppError> {
    let adapter = AdminLlmApplicationAdapter::new(app_state.as_ref());
    let service = adapter.llm_conversation_manage()?;

    match service.get_llm_conversation_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
