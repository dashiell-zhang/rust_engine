use application_model::shared::current_user::CurrentUser;
use application_model::task_center::models::{
    CreateScheduleTaskDto, EditTaskSettingDto, QueueTaskPageListDto, QueueTaskPageRequest,
    TaskSettingPageListDto, TaskSettingPageRequest,
};
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::task_center::QueueTaskManageService
    for crate::task_center::service::TaskCenterService<'_>
{
    async fn get_queue_task_list(
        &self,
        params: QueueTaskPageRequest,
    ) -> Result<QueueTaskPageListDto, AppError> {
        crate::task_center::service::TaskCenterService::get_queue_task_list(self, params).await
    }
    async fn retry_queue_task(&self, id: i64) -> Result<bool, AppError> {
        crate::task_center::service::TaskCenterService::retry_queue_task(self, id).await
    }
}

#[async_trait]
impl application_interface::services::task_center::TaskSettingService
    for crate::task_center::service::TaskCenterService<'_>
{
    async fn get_task_setting_list(
        &self,
        params: TaskSettingPageRequest,
    ) -> Result<TaskSettingPageListDto, AppError> {
        crate::task_center::service::TaskCenterService::get_task_setting_list(self, params).await
    }
    async fn update_task_setting(
        &self,
        id: i64,
        dto: EditTaskSettingDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::task_center::service::TaskCenterService::update_task_setting(
            self,
            id,
            dto,
            current_user,
        )
        .await
    }
    async fn get_args_schedule_task_name_list(&self) -> Result<Vec<String>, AppError> {
        crate::task_center::service::TaskCenterService::get_args_schedule_task_name_list(self).await
    }
    async fn create_schedule_task(
        &self,
        dto: CreateScheduleTaskDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        crate::task_center::service::TaskCenterService::create_schedule_task(
            self,
            dto,
            current_user,
        )
        .await
    }
    async fn delete_task_setting(
        &self,
        id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::task_center::service::TaskCenterService::delete_task_setting(self, id, current_user)
            .await
    }
}
