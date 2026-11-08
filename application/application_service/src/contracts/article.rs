use application_model::article::models::{
    ArticleClickDto, ArticleDto, ArticlePageListDto, CategoryDto, CategoryPageListDto,
    CategoryPageRequest, CategorySelectDto, EditArticleDto, EditCategoryDto,
    PublicArticleDetailDto, PublicArticleListItemDto, PublicArticleNavigationDto,
    PublicArticlePageListDto, PublicArticlePageRequest, PublicCategoryDto,
    PublicRecommendArticleRequest,
};
use application_model::shared::current_user::CurrentUser;
use application_model::shared::models::PageRequestDto;
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::article::ArticleService
    for crate::article::service::ArticleWebApiService<'_>
{
    async fn get_category_list(
        &self,
        params: CategoryPageRequest,
    ) -> Result<CategoryPageListDto, AppError> {
        crate::article::service::ArticleWebApiService::get_category_list(self, params).await
    }
    async fn get_category(&self, category_id: i64) -> Result<Option<CategoryDto>, AppError> {
        crate::article::service::ArticleWebApiService::get_category(self, category_id).await
    }
    async fn create_category(
        &self,
        dto: EditCategoryDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        crate::article::service::ArticleWebApiService::create_category(self, dto, current_user)
            .await
    }
    async fn update_category(
        &self,
        category_id: i64,
        dto: EditCategoryDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::article::service::ArticleWebApiService::update_category(
            self,
            category_id,
            dto,
            current_user,
        )
        .await
    }
    async fn delete_category(
        &self,
        category_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::article::service::ArticleWebApiService::delete_category(
            self,
            category_id,
            current_user,
        )
        .await
    }
    async fn get_article_list(
        &self,
        params: PageRequestDto,
    ) -> Result<ArticlePageListDto, AppError> {
        crate::article::service::ArticleWebApiService::get_article_list(self, params).await
    }
    async fn get_article(&self, article_id: i64) -> Result<Option<ArticleDto>, AppError> {
        crate::article::service::ArticleWebApiService::get_article(self, article_id).await
    }
    async fn create_article(
        &self,
        dto: EditArticleDto,
        upload_key: i64,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        crate::article::service::ArticleWebApiService::create_article(
            self,
            dto,
            upload_key,
            current_user,
        )
        .await
    }
    async fn update_article(
        &self,
        article_id: i64,
        dto: EditArticleDto,
        upload_key: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::article::service::ArticleWebApiService::update_article(
            self,
            article_id,
            dto,
            upload_key,
            current_user,
        )
        .await
    }
    async fn delete_article(
        &self,
        article_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::article::service::ArticleWebApiService::delete_article(
            self,
            article_id,
            current_user,
        )
        .await
    }
    async fn get_public_article_list(
        &self,
        params: PublicArticlePageRequest,
    ) -> Result<PublicArticlePageListDto, AppError> {
        crate::article::service::ArticleWebApiService::get_public_article_list(self, params).await
    }
    async fn get_public_article(
        &self,
        article_id: i64,
    ) -> Result<Option<PublicArticleDetailDto>, AppError> {
        crate::article::service::ArticleWebApiService::get_public_article(self, article_id).await
    }
    async fn get_public_recommend_article_list(
        &self,
        params: PublicRecommendArticleRequest,
    ) -> Result<Vec<PublicArticleListItemDto>, AppError> {
        crate::article::service::ArticleWebApiService::get_public_recommend_article_list(
            self, params,
        )
        .await
    }
    async fn increase_public_article_click_count(
        &self,
        article_id: i64,
    ) -> Result<Option<ArticleClickDto>, AppError> {
        crate::article::service::ArticleWebApiService::increase_public_article_click_count(
            self, article_id,
        )
        .await
    }
    async fn get_public_article_navigation(
        &self,
        article_id: i64,
    ) -> Result<Option<PublicArticleNavigationDto>, AppError> {
        crate::article::service::ArticleWebApiService::get_public_article_navigation(
            self, article_id,
        )
        .await
    }
    async fn get_category_select_list(
        &self,
        parent_id: Option<i64>,
    ) -> Result<Vec<CategorySelectDto>, AppError> {
        crate::article::service::ArticleWebApiService::get_category_select_list(self, parent_id)
            .await
    }
    async fn get_public_category_tree(
        &self,
        parent_id: Option<i64>,
    ) -> Result<Vec<PublicCategoryDto>, AppError> {
        crate::article::service::ArticleWebApiService::get_public_category_tree(self, parent_id)
            .await
    }
}
