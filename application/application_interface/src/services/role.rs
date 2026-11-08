use application_model::role::models::{
    EditRoleDto, RoleDto, RoleFunctionDto, RolePageListDto, RolePageRequest, RoleSelectDto,
    SetRoleFunctionDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

/// 角色与角色授权应用契约。
#[async_trait]
pub trait RoleService: Send + Sync {
    async fn get_role_list(&self, params: RolePageRequest) -> Result<RolePageListDto, AppError>;
    async fn get_role(&self, role_id: i64) -> Result<Option<RoleDto>, AppError>;
    async fn create_role(&self, dto: EditRoleDto) -> Result<String, AppError>;
    async fn update_role(&self, role_id: i64, dto: EditRoleDto) -> Result<bool, AppError>;
    async fn delete_role(&self, role_id: i64) -> Result<bool, AppError>;
    async fn get_role_function(&self, role_id: i64) -> Result<Vec<RoleFunctionDto>, AppError>;
    async fn set_role_function(
        &self,
        dto: SetRoleFunctionDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn get_role_select(&self) -> Result<Vec<RoleSelectDto>, AppError>;
}
