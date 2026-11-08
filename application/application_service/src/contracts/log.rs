use application_model::log::models::{LogPageListDto, LogPageRequest};
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::log::LogManageService
    for crate::log::service::LogManageService<'_>
{
    async fn get_log_list(&self, params: LogPageRequest) -> Result<LogPageListDto, AppError> {
        crate::log::service::LogManageService::get_log_list(self, params).await
    }
}
