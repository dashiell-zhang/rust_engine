use crate::openapi::article::ArticleApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 文章管理特性
pub struct ArticleFeature;

impl WebApiFeature for ArticleFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::Article>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::article::get_category_list),
            webapi_core::endpoint!(crate::handlers::article::get_category_select_list),
            webapi_core::endpoint!(crate::handlers::article::get_category),
            webapi_core::endpoint!(crate::handlers::article::create_category),
            webapi_core::endpoint!(crate::handlers::article::update_category),
            webapi_core::endpoint!(crate::handlers::article::delete_category),
            webapi_core::endpoint!(crate::handlers::article::get_article_list),
            webapi_core::endpoint!(crate::handlers::article::get_article),
            webapi_core::endpoint!(crate::handlers::article::create_article),
            webapi_core::endpoint!(crate::handlers::article::update_article),
            webapi_core::endpoint!(crate::handlers::article::delete_article),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(ArticleApiDoc::openapi())
    }
}
