use application_model::link::models::{
    EditLinkDto, LinkDto, LinkPageListDto, LinkPageRequest, PublicLinkDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

/// 友情链接应用契约。
#[async_trait]
pub trait LinkService: Send + Sync {
    async fn get_link_list(&self, params: LinkPageRequest) -> Result<LinkPageListDto, AppError>;
    async fn get_link(&self, link_id: i64) -> Result<Option<LinkDto>, AppError>;
    async fn get_public_link_list(&self) -> Result<Vec<PublicLinkDto>, AppError>;
    async fn create_link(
        &self,
        dto: EditLinkDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>;
    async fn update_link(
        &self,
        link_id: i64,
        dto: EditLinkDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn delete_link(&self, link_id: i64, current_user: &CurrentUser)
    -> Result<bool, AppError>;
}
