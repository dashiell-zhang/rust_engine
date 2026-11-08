use crate::openapi::base::BaseApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 基础工具特性
///
/// 提供二维码生成、雪花 ID 获取等工具接口
/// 管理端接口不要求登录鉴权，但需要通过请求签名验证
pub struct BaseFeature;

impl WebApiFeature for BaseFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::Base>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::base::get_region),
            webapi_core::endpoint!(crate::handlers::base::get_region_all),
            webapi_core::endpoint!(crate::handlers::base::get_qr_code),
            webapi_core::endpoint!(crate::handlers::base::get_value_list),
            webapi_core::endpoint!(crate::handlers::base::get_snowflake_id),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(BaseApiDoc::openapi())
    }
}
