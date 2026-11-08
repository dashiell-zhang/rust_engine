use application_model::app_setting::models::{
    AppSettingDto, DeleteAppSettingQuery, GetAppSettingListQuery, GetAppSettingQuery,
    SetAppSettingDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;
use std::collections::HashMap;

#[async_trait]
impl application_interface::services::app_setting::AppSettingService
    for crate::app_setting::service::AppSettingService<'_>
{
    async fn get_setting_list(
        &self,
        query: GetAppSettingListQuery,
    ) -> Result<Vec<AppSettingDto>, AppError> {
        crate::app_setting::service::AppSettingService::get_setting_list(self, query).await
    }
    async fn get_setting(
        &self,
        query: GetAppSettingQuery,
    ) -> Result<Option<AppSettingDto>, AppError> {
        crate::app_setting::service::AppSettingService::get_setting(self, query).await
    }
    async fn get_value_list(&self, group_id: i64) -> Result<HashMap<String, String>, AppError> {
        crate::app_setting::service::AppSettingService::get_value_list(self, group_id).await
    }
    async fn set_setting(
        &self,
        dto: SetAppSettingDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::app_setting::service::AppSettingService::set_setting(self, dto, current_user).await
    }
    async fn delete_setting(
        &self,
        query: DeleteAppSettingQuery,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::app_setting::service::AppSettingService::delete_setting(self, query, current_user)
            .await
    }
    async fn get_module_list(&self) -> Result<Vec<String>, AppError> {
        crate::app_setting::service::AppSettingService::get_module_list(self).await
    }
}
