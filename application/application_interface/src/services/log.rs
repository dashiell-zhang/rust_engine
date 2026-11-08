use application_model::log::models::{LogPageListDto, LogPageRequest};
use async_trait::async_trait;
use common::app_error::AppError;

/// 管理端日志查询应用契约。
#[async_trait]
pub trait LogManageService: Send + Sync {
    async fn get_log_list(&self, params: LogPageRequest) -> Result<LogPageListDto, AppError>;
}
