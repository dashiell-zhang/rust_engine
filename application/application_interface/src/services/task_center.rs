use application_model::shared::current_user::CurrentUser;
use application_model::task_center::models::{
    CreateScheduleTaskDto, EditTaskSettingDto, QueueTaskPageListDto, QueueTaskPageRequest,
    TaskSettingPageListDto, TaskSettingPageRequest,
};
use async_trait::async_trait;
use common::app_error::AppError;

/// 队列查询与手工重试应用契约。
#[async_trait]
pub trait QueueTaskManageService: Send + Sync {
    async fn get_queue_task_list(
        &self,
        params: QueueTaskPageRequest,
    ) -> Result<QueueTaskPageListDto, AppError>;
    async fn retry_queue_task(&self, id: i64) -> Result<bool, AppError>;
}

/// 任务配置管理应用契约。
#[async_trait]
pub trait TaskSettingService: Send + Sync {
    async fn get_task_setting_list(
        &self,
        params: TaskSettingPageRequest,
    ) -> Result<TaskSettingPageListDto, AppError>;
    async fn update_task_setting(
        &self,
        id: i64,
        dto: EditTaskSettingDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn get_args_schedule_task_name_list(&self) -> Result<Vec<String>, AppError>;
    async fn create_schedule_task(
        &self,
        dto: CreateScheduleTaskDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>;
    async fn delete_task_setting(
        &self,
        id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
}

/// 任务中心装配入口，查询重试和配置管理仍可独立依赖。
pub trait TaskCenterService: QueueTaskManageService + TaskSettingService {}
impl<T: QueueTaskManageService + TaskSettingService + ?Sized> TaskCenterService for T {}
