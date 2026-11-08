use crate::openapi::authorize::AuthorizeApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::cache_data::{CacheDataOptions, cache_method_router_with_options};
use webapi_core::host::features::WebApiFeature;
use webapi_core::queue_limit::{QueueLimitOptions, queue_limit_method_router};

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
            webapi_core::endpoint!(crate::handlers::authorize::update_route),
            webapi_core::endpoint!(crate::handlers::authorize::get_public_key),
            webapi_core::endpoint!(crate::handlers::authorize::get_token),
            webapi_core::endpoint!(crate::handlers::authorize::get_token_by_sms),
            webapi_core::endpoint!(crate::handlers::authorize::get_token_by_wx_mini_app),
            webapi_core::endpoint!(crate::handlers::authorize::get_token_by_wx_app),
            webapi_core::endpoint!(crate::handlers::authorize::send_sms_verify_code),
            webapi_core::endpoint!(crate::handlers::authorize::get_function_list).configure_action(
                |route, app_state| {
                    cache_method_router_with_options(
                        route,
                        app_state.clone(),
                        CacheDataOptions::with_authorization(60),
                        concat!(module_path!(), "::get_function_list"),
                    )
                },
            ),
            webapi_core::endpoint!(crate::handlers::authorize::update_password_by_sms)
                .configure_action(|route, app_state| {
                    queue_limit_method_router(
                        route,
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::rejecting_with_authorization(1),
                        concat!(module_path!(), "::update_password_by_sms"),
                    )
                }),
            webapi_core::endpoint!(crate::handlers::authorize::update_password_by_old_password)
                .configure_action(|route, app_state| {
                    queue_limit_method_router(
                        route,
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::rejecting_with_authorization(1),
                        concat!(module_path!(), "::update_password_by_old_password"),
                    )
                }),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(AuthorizeApiDoc::openapi())
    }
}
