use application_model::shared::current_user::CurrentUser;
use application_model::site::models::{EditSiteDto, SiteDto};
use async_trait::async_trait;
use common::app_error::AppError;

/// 站点信息应用契约。
#[async_trait]
pub trait SiteService: Send + Sync {
    async fn get_site(&self) -> Result<SiteDto, AppError>;
    async fn update_site(
        &self,
        dto: EditSiteDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
}
