use application_model::log::models::{LogDto, LogPageRequest};
use application_model::shared::models::PageListDto;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use webapi_core::extract::{Json, PageQuery};
use webapi_core::{auth::UserContext, error::AppError};
use webapi_support::WebApiApplicationExt;

use crate::state::AppState;

/// 获取日志分页列表
#[webapi_core::webapi_endpoint(
    policy = authorized_signed,
    permission = "获取日志列表",
    get,
    path = "/Operations/GetLogList",
    params(LogPageRequest),
    responses(
        (status = 200, description = "查询成功", body = PageListDto<LogDto>),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "日志管理"
)]
pub async fn get_log_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<LogPageRequest>,
) -> Result<Response, AppError> {
    let service = app_state.services().log_manage()?;

    match service.get_log_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
