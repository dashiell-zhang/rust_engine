use application_model::role::models::{
    EditRoleDto, RoleDto, RoleFunctionDto, RolePageListDto, RolePageRequest, RoleSelectDto,
    SetRoleFunctionDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::role::RoleService for crate::role::service::RoleService<'_> {
    async fn get_role_list(&self, params: RolePageRequest) -> Result<RolePageListDto, AppError> {
        crate::role::service::RoleService::get_role_list(self, params).await
    }
    async fn get_role(&self, role_id: i64) -> Result<Option<RoleDto>, AppError> {
        crate::role::service::RoleService::get_role(self, role_id).await
    }
    async fn create_role(&self, dto: EditRoleDto) -> Result<String, AppError> {
        crate::role::service::RoleService::create_role(self, dto).await
    }
    async fn update_role(&self, role_id: i64, dto: EditRoleDto) -> Result<bool, AppError> {
        crate::role::service::RoleService::update_role(self, role_id, dto).await
    }
    async fn delete_role(&self, role_id: i64) -> Result<bool, AppError> {
        crate::role::service::RoleService::delete_role(self, role_id).await
    }
    async fn get_role_function(&self, role_id: i64) -> Result<Vec<RoleFunctionDto>, AppError> {
        crate::role::service::RoleService::get_role_function(self, role_id).await
    }
    async fn set_role_function(
        &self,
        dto: SetRoleFunctionDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::role::service::RoleService::set_role_function(self, dto, current_user).await
    }
    async fn get_role_select(&self) -> Result<Vec<RoleSelectDto>, AppError> {
        crate::role::service::RoleService::get_role_select(self).await
    }
}
