use crate::openapi::role::RoleApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;
use webapi_core::queue_limit::{QueueLimitOptions, queue_limit_method_router};

/// 角色管理特性
pub struct RoleFeature;

impl WebApiFeature for RoleFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::Role>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::role::get_role_list),
            webapi_core::endpoint!(crate::handlers::role::get_role),
            webapi_core::endpoint!(crate::handlers::role::create_role).configure_action(
                |route, app_state| {
                    queue_limit_method_router(
                        route,
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::rejecting_with_authorization(1),
                        concat!(module_path!(), "::create_role"),
                    )
                },
            ),
            webapi_core::endpoint!(crate::handlers::role::update_role).configure_action(
                |route, app_state| {
                    queue_limit_method_router(
                        route,
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::rejecting_with_authorization(1),
                        concat!(module_path!(), "::update_role"),
                    )
                },
            ),
            webapi_core::endpoint!(crate::handlers::role::delete_role),
            webapi_core::endpoint!(crate::handlers::role::get_role_function),
            webapi_core::endpoint!(crate::handlers::role::set_role_function),
            webapi_core::endpoint!(crate::handlers::role::get_role_select),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(RoleApiDoc::openapi())
    }
}
