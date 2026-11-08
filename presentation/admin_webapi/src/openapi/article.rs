use application_model::article::models::{
    ArticleDto, CategoryDto, CategorySelectDto, EditArticleDto, EditCategoryDto,
};
use application_model::shared::models::PageListDto;
use utoipa::OpenApi;

use crate::handlers::article;

/// 文章管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        article::get_category_list,
        article::get_category_select_list,
        article::get_category,
        article::create_category,
        article::update_category,
        article::delete_category,
        article::get_article_list,
        article::get_article,
        article::create_article,
        article::update_article,
        article::delete_article,
    ),
    components(
        schemas(
            PageListDto<CategoryDto>,
            CategoryDto,
            CategorySelectDto,
            EditCategoryDto,
            PageListDto<ArticleDto>,
            ArticleDto,
            EditArticleDto,
        )
    ),
    tags(
        (name = "文章管理", description = "栏目与文章内容管理接口"),
    )
)]
pub struct ArticleApiDoc;
