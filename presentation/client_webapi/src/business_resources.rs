use std::{future::Future, pin::Pin, sync::Arc};
use webapi_core::host::{resources::WebApiResources, settings::Settings};
use webapi_support::services;
/// 当前宿主只选择其启用功能使用的业务实现。
pub fn build(
    settings: &Settings,
    check_log_read_health: bool,
    http_clients: Arc<http_client::HttpClientFactory>,
) -> Pin<Box<dyn Future<Output = Result<WebApiResources, String>> + Send + '_>> {
    Box::pin(async move {
        let file_storage = crate::host_support::build_file_storage(settings, http_clients.clone())?;
        let mut resources =
            webapi_support::resources::build_resources(settings, check_log_read_health).await?;
        resources.insert(webapi_support::resources::FileStorageResource(file_storage))?;
        webapi_support::llm_resources::register_default_client_factory(
            &mut resources,
            http_clients,
        )?;
        webapi_support::llm_resources::register_default_model_config_resolver(&mut resources)?;
        services::Authorize::register_default(&mut resources)?;
        services::PermissionRoute::register_default(&mut resources)?;
        services::Base::register_default(&mut resources)?;
        services::User::register_default(&mut resources)?;
        services::File::register_default(&mut resources)?;
        services::Pay::register_default(&mut resources)?;
        services::LlmInvoke::register_default(&mut resources)?;
        Ok(resources)
    })
}
