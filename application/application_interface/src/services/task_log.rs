use application_model::task_center::models::{
    TaskExecutionLogPageListDto, TaskExecutionLogPageRequest,
};
use async_trait::async_trait;
use common::app_error::AppError;

/// 任务执行日志查询应用契约。
#[async_trait]
pub trait TaskExecutionLogService: Send + Sync {
    async fn get_task_execution_log_list(
        &self,
        params: TaskExecutionLogPageRequest,
    ) -> Result<TaskExecutionLogPageListDto, AppError>;
}
