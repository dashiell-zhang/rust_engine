use application_model::shared::models::PageListDto;
use application_model::task_center::models::{
    CreateScheduleTaskDto, EditTaskSettingDto, QueueTaskDto, QueueTaskIdQuery,
    QueueTaskPageRequest, TaskSettingDto, TaskSettingIdQuery, TaskSettingPageRequest,
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

/// 获取任务配置列表
#[utoipa::path(
    get,
    path = "/Operations/GetTaskSettingList",
    params(TaskSettingPageRequest),
    responses(
        (status = 200, description = "查询成功", body = PageListDto<TaskSettingDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "任务中心"
)]
pub async fn get_task_setting_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<TaskSettingPageRequest>,
) -> Result<Response, AppError> {
    let service = app_state.services().task_center()?;

    match service.get_task_setting_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新任务配置信息
#[utoipa::path(
    post,
    path = "/Operations/UpdateTaskSetting",
    params(TaskSettingIdQuery),
    request_body = EditTaskSettingDto,
    responses(
        (status = 200, description = "更新成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "任务中心"
)]
pub async fn update_task_setting(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<TaskSettingIdQuery>,
    Json(dto): Json<EditTaskSettingDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().task_center()?;
    let current_user = user_ctx.current_user();

    match service
        .update_task_setting(query.task_setting_id, dto, &current_user)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取支持参数的定时任务名称列表
#[utoipa::path(
    get,
    path = "/Operations/GetArgsScheduleTaskNameList",
    responses(
        (status = 200, description = "查询成功", body = Vec<String>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "任务中心"
)]
pub async fn get_args_schedule_task_name_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
) -> Result<Response, AppError> {
    let service = app_state.services().task_center()?;

    match service.get_args_schedule_task_name_list().await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建带参定时任务
#[utoipa::path(
    post,
    path = "/Operations/CreateScheduleTask",
    request_body = CreateScheduleTaskDto,
    responses(
        (status = 200, description = "创建成功，返回新任务配置 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "任务中心"
)]
pub async fn create_schedule_task(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<CreateScheduleTaskDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().task_center()?;
    let current_user = user_ctx.current_user();

    match service.create_schedule_task(dto, &current_user).await {
        Ok(task_setting_id) => Ok((StatusCode::OK, task_setting_id).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取队列任务列表
#[utoipa::path(
    get,
    path = "/Operations/GetQueueTaskList",
    params(QueueTaskPageRequest),
    responses(
        (status = 200, description = "查询成功", body = PageListDto<QueueTaskDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "任务中心"
)]
pub async fn get_queue_task_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<QueueTaskPageRequest>,
) -> Result<Response, AppError> {
    let service = app_state.services().task_center()?;

    match service.get_queue_task_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 重试队列任务
#[utoipa::path(
    post,
    path = "/Operations/RetryQueueTask",
    params(QueueTaskIdQuery),
    responses(
        (status = 200, description = "重试成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "任务中心"
)]
pub async fn retry_queue_task(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<QueueTaskIdQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().task_center()?;

    match service.retry_queue_task(query.id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
