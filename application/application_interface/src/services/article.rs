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

/// 文章与栏目应用契约。
#[async_trait]
pub trait ArticleService: Send + Sync {
    async fn get_category_list(
        &self,
        params: CategoryPageRequest,
    ) -> Result<CategoryPageListDto, AppError>;
    async fn get_category(&self, category_id: i64) -> Result<Option<CategoryDto>, AppError>;
    async fn create_category(
        &self,
        dto: EditCategoryDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>;
    async fn update_category(
        &self,
        category_id: i64,
        dto: EditCategoryDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn delete_category(
        &self,
        category_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn get_article_list(
        &self,
        params: PageRequestDto,
    ) -> Result<ArticlePageListDto, AppError>;
    async fn get_article(&self, article_id: i64) -> Result<Option<ArticleDto>, AppError>;
    async fn create_article(
        &self,
        dto: EditArticleDto,
        upload_key: i64,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>;
    async fn update_article(
        &self,
        article_id: i64,
        dto: EditArticleDto,
        upload_key: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn delete_article(
        &self,
        article_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn get_public_article_list(
        &self,
        params: PublicArticlePageRequest,
    ) -> Result<PublicArticlePageListDto, AppError>;
    async fn get_public_article(
        &self,
        article_id: i64,
    ) -> Result<Option<PublicArticleDetailDto>, AppError>;
    async fn get_public_recommend_article_list(
        &self,
        params: PublicRecommendArticleRequest,
    ) -> Result<Vec<PublicArticleListItemDto>, AppError>;
    async fn increase_public_article_click_count(
        &self,
        article_id: i64,
    ) -> Result<Option<ArticleClickDto>, AppError>;
    async fn get_public_article_navigation(
        &self,
        article_id: i64,
    ) -> Result<Option<PublicArticleNavigationDto>, AppError>;
    async fn get_category_select_list(
        &self,
        parent_id: Option<i64>,
    ) -> Result<Vec<CategorySelectDto>, AppError>;
    async fn get_public_category_tree(
        &self,
        parent_id: Option<i64>,
    ) -> Result<Vec<PublicCategoryDto>, AppError>;
}
