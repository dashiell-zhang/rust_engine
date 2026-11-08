use application_model::article::models::{
    ArticleDto, CategoryDto, CategorySelectDto, EditArticleDto, EditCategoryDto,
};
use application_model::shared::models::PageListDto;
use utoipa::OpenApi;

/// 文章管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
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
