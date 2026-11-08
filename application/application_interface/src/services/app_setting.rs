use application_model::app_setting::models::{
    AppSettingDto, DeleteAppSettingQuery, GetAppSettingListQuery, GetAppSettingQuery,
    SetAppSettingDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;
use std::collections::HashMap;

/// 应用配置与键值数据应用契约。
#[async_trait]
pub trait AppSettingService: Send + Sync {
    async fn get_setting_list(
        &self,
        query: GetAppSettingListQuery,
    ) -> Result<Vec<AppSettingDto>, AppError>;
    async fn get_setting(
        &self,
        query: GetAppSettingQuery,
    ) -> Result<Option<AppSettingDto>, AppError>;
    async fn get_value_list(&self, group_id: i64) -> Result<HashMap<String, String>, AppError>;
    async fn set_setting(
        &self,
        dto: SetAppSettingDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn delete_setting(
        &self,
        query: DeleteAppSettingQuery,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn get_module_list(&self) -> Result<Vec<String>, AppError>;
}
