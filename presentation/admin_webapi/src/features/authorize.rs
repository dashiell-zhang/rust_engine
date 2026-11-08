use crate::openapi::authorize::AuthorizeApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 授权特性
pub struct AuthorizeFeature;

impl WebApiFeature for AuthorizeFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::Authorize>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::authorize::get_token),
            webapi_core::endpoint!(crate::handlers::authorize::init_data),
            webapi_core::endpoint!(crate::handlers::authorize::get_function_list),
            webapi_core::endpoint!(crate::handlers::authorize::update_route),
            webapi_core::endpoint!(crate::handlers::authorize::generate_ecdsa_key_pair),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(AuthorizeApiDoc::openapi())
    }
}
