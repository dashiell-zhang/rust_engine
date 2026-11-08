use crate::openapi::site::SiteApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 站点管理特性
pub struct SiteFeature;

impl WebApiFeature for SiteFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::Site>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::site::get_server_info),
            webapi_core::endpoint!(crate::handlers::site::get_site),
            webapi_core::endpoint!(crate::handlers::site::update_site),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(SiteApiDoc::openapi())
    }
}
