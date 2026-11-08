use crate::openapi::link::LinkApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 友情链接管理特性
pub struct LinkFeature;

impl WebApiFeature for LinkFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::Link>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::link::get_link_list),
            webapi_core::endpoint!(crate::handlers::link::get_link),
            webapi_core::endpoint!(crate::handlers::link::create_link),
            webapi_core::endpoint!(crate::handlers::link::update_link),
            webapi_core::endpoint!(crate::handlers::link::delete_link),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(LinkApiDoc::openapi())
    }
}
