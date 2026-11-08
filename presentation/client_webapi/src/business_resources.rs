use std::{future::Future, pin::Pin};
use webapi_core::host::{resources::WebApiResources, settings::Settings};
use webapi_support::services;
/// 当前宿主只选择其启用功能使用的业务实现。
pub fn build(
    settings: &Settings,
    check_log_read_health: bool,
) -> Pin<Box<dyn Future<Output = Result<WebApiResources, String>> + Send + '_>> {
    Box::pin(async move {
        let mut resources =
            webapi_support::resources::build_resources(settings, check_log_read_health).await?;
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
