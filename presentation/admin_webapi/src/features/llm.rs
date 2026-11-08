use crate::openapi::llm::LlmApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 管理端 LLM 能力特性
pub struct LlmFeature;

impl WebApiFeature for LlmFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::LlmApp>(resources)?;
        webapi_support::services::require::<webapi_support::services::LlmModel>(resources)?;
        webapi_support::services::require::<webapi_support::services::LlmConversation>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::llm::get_llm_model_list),
            webapi_core::endpoint!(crate::handlers::llm::get_llm_model_select),
            webapi_core::endpoint!(crate::handlers::llm::create_llm_model),
            webapi_core::endpoint!(crate::handlers::llm::update_llm_model),
            webapi_core::endpoint!(crate::handlers::llm::delete_llm_model),
            webapi_core::endpoint!(crate::handlers::llm::get_llm_app_list),
            webapi_core::endpoint!(crate::handlers::llm::create_llm_app),
            webapi_core::endpoint!(crate::handlers::llm::update_llm_app),
            webapi_core::endpoint!(crate::handlers::llm::delete_llm_app),
            webapi_core::endpoint!(crate::handlers::llm::test_llm_app),
            webapi_core::endpoint!(crate::handlers::llm::get_llm_conversation_list),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(LlmApiDoc::openapi())
    }
}
