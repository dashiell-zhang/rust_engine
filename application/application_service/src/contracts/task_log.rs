use application_model::task_center::models::{
    TaskExecutionLogPageListDto, TaskExecutionLogPageRequest,
};
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::task_log::TaskExecutionLogService
    for crate::task_center::log_service::TaskExecutionLogService<'_>
{
    async fn get_task_execution_log_list(
        &self,
        params: TaskExecutionLogPageRequest,
    ) -> Result<TaskExecutionLogPageListDto, AppError> {
        crate::task_center::log_service::TaskExecutionLogService::get_task_execution_log_list(
            self, params,
        )
        .await
    }
}
