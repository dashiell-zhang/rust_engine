use application_model::region::models::RegionDto;
use application_model::shared::current_user::CurrentUser;
use application_model::shared::models::PageListDto;
use application_model::user::models::{
    EditUserDto, EditUserPhoneBySmsDto, EditUserProfileDto, SetUserFunctionDto, SetUserRoleDto,
    UserDto, UserFunctionDto, UserPageRequest, UserProfileDto, UserRoleDto,
};
use async_trait::async_trait;
use common::app_error::AppError;

/// 用户、资料与用户授权应用契约。
#[async_trait]
pub trait UserService: Send + Sync {
    async fn get_user_list(
        &self,
        params: UserPageRequest,
    ) -> Result<PageListDto<UserDto>, AppError>;
    async fn get_user(&self, target_id: i64) -> Result<Option<UserDto>, AppError>;
    async fn get_user_role_list(&self, target_id: i64) -> Result<Vec<UserRoleDto>, AppError>;
    async fn set_user_role(
        &self,
        dto: SetUserRoleDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn get_user_function(&self, target_id: i64) -> Result<Vec<UserFunctionDto>, AppError>;
    async fn set_user_function(
        &self,
        dto: SetUserFunctionDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn create_user(
        &self,
        dto: EditUserDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>;
    async fn update_user(
        &self,
        target_id: i64,
        dto: EditUserDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn delete_user(
        &self,
        target_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn get_user_profile(&self, target_id: i64) -> Result<Option<UserProfileDto>, AppError>;
    async fn update_user_profile(
        &self,
        target_id: i64,
        dto: EditUserProfileDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn get_region_province_list(&self) -> Result<Vec<RegionDto>, AppError>;
    async fn get_region_city_list(&self, province_id: i32) -> Result<Vec<RegionDto>, AppError>;
    async fn get_region_area_list(&self, city_id: i32) -> Result<Vec<RegionDto>, AppError>;
    async fn edit_user_phone_by_sms(
        &self,
        request: EditUserPhoneBySmsDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
}
