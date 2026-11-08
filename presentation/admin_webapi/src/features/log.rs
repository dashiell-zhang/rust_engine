use crate::openapi::log::LogApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 日志管理特性
pub struct LogFeature;

impl WebApiFeature for LogFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::LogManage>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![webapi_core::endpoint!(crate::handlers::log::get_log_list)]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(LogApiDoc::openapi())
    }
}
