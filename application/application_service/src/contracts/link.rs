use application_model::link::models::{
    EditLinkDto, LinkDto, LinkPageListDto, LinkPageRequest, PublicLinkDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::link::LinkService
    for crate::link::service::LinkWebApiService<'_>
{
    async fn get_link_list(&self, params: LinkPageRequest) -> Result<LinkPageListDto, AppError> {
        crate::link::service::LinkWebApiService::get_link_list(self, params).await
    }
    async fn get_link(&self, link_id: i64) -> Result<Option<LinkDto>, AppError> {
        crate::link::service::LinkWebApiService::get_link(self, link_id).await
    }
    async fn get_public_link_list(&self) -> Result<Vec<PublicLinkDto>, AppError> {
        crate::link::service::LinkWebApiService::get_public_link_list(self).await
    }
    async fn create_link(
        &self,
        dto: EditLinkDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        crate::link::service::LinkWebApiService::create_link(self, dto, current_user).await
    }
    async fn update_link(
        &self,
        link_id: i64,
        dto: EditLinkDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::link::service::LinkWebApiService::update_link(self, link_id, dto, current_user).await
    }
    async fn delete_link(
        &self,
        link_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::link::service::LinkWebApiService::delete_link(self, link_id, current_user).await
    }
}
