use crate::openapi::demo::DemoApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 客户端服务代理与 LLM 能力演示特性
pub struct DemoFeature;

impl WebApiFeature for DemoFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::LlmInvoke>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::demo::echo),
            webapi_core::endpoint!(crate::handlers::demo::add),
            webapi_core::endpoint!(crate::handlers::demo::echo2),
            webapi_core::endpoint!(crate::handlers::demo::add2),
            webapi_core::endpoint!(crate::handlers::demo::a_ref),
            webapi_core::endpoint!(crate::handlers::demo::stream_numbers),
            webapi_core::endpoint!(crate::handlers::demo::stream_numbers2),
            webapi_core::endpoint!(crate::handlers::demo::test_llm),
            webapi_core::endpoint!(crate::handlers::demo::test_llm_stream),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(DemoApiDoc::openapi())
    }
}
