//! LLM 的两项运行依赖单独注册，替换一项不影响另一项或业务服务选择。
use crate::resources::BusinessResources;
use std::sync::Arc;
use webapi_core::host::resources::WebApiResources;

pub struct LlmClientFactoryResource(pub Arc<dyn llm::LlmClientFactory>);
pub struct LlmModelConfigResource(pub Arc<dyn llm::LlmModelConfigResolver>);

pub fn register_default_client_factory(
    resources: &mut WebApiResources,
    http_clients: Arc<http_client::HttpClientFactory>,
) -> Result<(), String> {
    resources.insert(LlmClientFactoryResource(Arc::new(
        llm::DynamicLlmClientFactory::new(http_clients),
    )))
}
pub fn register_default_model_config_resolver(
    resources: &mut WebApiResources,
) -> Result<(), String> {
    let db = resources
        .get::<BusinessResources>()?
        .modules
        .get::<repository::BusinessModule>()?
        .write_pool();
    resources.insert(LlmModelConfigResource(Arc::new(
        application_service_llm::llm::model_config_resolver::DatabaseLlmModelConfigResolver::new(
            db,
        ),
    )))
}
