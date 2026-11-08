use application_model::shared::current_user::CurrentUser;
use application_model::site::models::{EditSiteDto, SiteDto};
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::site::SiteService
    for crate::site::service::SiteWebApiService<'_>
{
    async fn get_site(&self) -> Result<SiteDto, AppError> {
        crate::site::service::SiteWebApiService::get_site(self).await
    }
    async fn update_site(
        &self,
        dto: EditSiteDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::site::service::SiteWebApiService::update_site(self, dto, current_user).await
    }
}
