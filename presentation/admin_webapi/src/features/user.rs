use crate::openapi::user::UserApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;
use webapi_core::queue_limit::{QueueLimitOptions, queue_limit_method_router};

/// 用户管理特性
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
            webapi_core::endpoint!(crate::handlers::user::get_user_list),
            webapi_core::endpoint!(crate::handlers::user::get_user),
            webapi_core::endpoint!(crate::handlers::user::create_user),
            webapi_core::endpoint!(crate::handlers::user::update_user),
            webapi_core::endpoint!(crate::handlers::user::delete_user),
            webapi_core::endpoint!(crate::handlers::user::get_user_role_list),
            webapi_core::endpoint!(crate::handlers::user::set_user_role).configure_action(
                |route, app_state| {
                    queue_limit_method_router(
                        route,
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::new(1),
                        concat!(module_path!(), "::set_user_role"),
                    )
                },
            ),
            webapi_core::endpoint!(crate::handlers::user::get_user_function),
            webapi_core::endpoint!(crate::handlers::user::set_user_function).configure_action(
                |route, app_state| {
                    queue_limit_method_router(
                        route,
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::new(1),
                        concat!(module_path!(), "::set_user_function"),
                    )
                },
            ),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(UserApiDoc::openapi())
    }
}
