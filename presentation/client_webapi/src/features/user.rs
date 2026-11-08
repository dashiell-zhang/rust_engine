use crate::openapi::user::UserApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::cache_data::{CacheDataOptions, cache_method_router_with_options};
use webapi_core::host::features::WebApiFeature;

/// 用户特性
pub struct UserFeature;

impl WebApiFeature for UserFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::User>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::user::get_user).configure_action(
                |route, app_state| {
                    cache_method_router_with_options(
                        route,
                        app_state.clone(),
                        CacheDataOptions::with_authorization(60),
                        concat!(module_path!(), "::get_user"),
                    )
                },
            ),
            webapi_core::endpoint!(crate::handlers::user::edit_user_phone_by_sms),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(UserApiDoc::openapi())
    }
}
