use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::queue_limit::{QueueLimitOptions, queue_limit_method_router};
use webapi_core::routes::WebApiRoutes as Router;

use crate::{
    features::protect_method_router,
    handlers::user::{
        create_user, delete_user, get_user, get_user_function, get_user_list, get_user_role_list,
        set_user_function, set_user_role, update_user,
    },
    openapi::user::UserApiDoc,
    state::AppState,
};

use super::{WebApiFeature, WebApiRoutePermission};

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
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/User/GetUserList",
                protect_method_router(
                    webapi_core::routes::get(get_user_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/GetUserList",
                    false,
                ),
            )
            .route(
                "/User/GetUser",
                protect_method_router(
                    webapi_core::routes::get(get_user),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/GetUser",
                    false,
                ),
            )
            .route(
                "/User/CreateUser",
                protect_method_router(
                    webapi_core::routes::post(create_user).json_body("createUser"),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/CreateUser",
                    false,
                ),
            )
            .route(
                "/User/UpdateUser",
                protect_method_router(
                    webapi_core::routes::post(update_user).json_body("updateUser"),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/UpdateUser",
                    false,
                ),
            )
            .route(
                "/User/DeleteUser",
                protect_method_router(
                    webapi_core::routes::delete(delete_user),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/DeleteUser",
                    false,
                ),
            )
            .route(
                "/User/GetUserRoleList",
                protect_method_router(
                    webapi_core::routes::get(get_user_role_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/GetUserRoleList",
                    false,
                ),
            )
            .route(
                "/User/SetUserRole",
                protect_method_router(
                    queue_limit_method_router(
                        webapi_core::routes::post(set_user_role).json_body("setUserRole"),
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::new(1),
                        concat!(module_path!(), "::set_user_role"),
                    ),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/SetUserRole",
                    false,
                ),
            )
            .route(
                "/User/GetUserFunction",
                protect_method_router(
                    webapi_core::routes::get(get_user_function),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/GetUserFunction",
                    false,
                ),
            )
            .route(
                "/User/SetUserFunction",
                protect_method_router(
                    queue_limit_method_router(
                        webapi_core::routes::post(set_user_function).json_body("setUserFunction"),
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::new(1),
                        concat!(module_path!(), "::set_user_function"),
                    ),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/SetUserFunction",
                    false,
                ),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(UserApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![
            WebApiRoutePermission {
                route: "/User/GetUserList",
                remark: "获取用户列表",
            },
            WebApiRoutePermission {
                route: "/User/GetUser",
                remark: "通过 UserId 获取用户信息",
            },
            WebApiRoutePermission {
                route: "/User/CreateUser",
                remark: "创建用户",
            },
            WebApiRoutePermission {
                route: "/User/UpdateUser",
                remark: "更新用户信息",
            },
            WebApiRoutePermission {
                route: "/User/DeleteUser",
                remark: "删除用户",
            },
            WebApiRoutePermission {
                route: "/User/GetUserRoleList",
                remark: "获取用户角色列表",
            },
            WebApiRoutePermission {
                route: "/User/SetUserRole",
                remark: "设置用户角色",
            },
            WebApiRoutePermission {
                route: "/User/GetUserFunction",
                remark: "获取某个用户的功能权限",
            },
            WebApiRoutePermission {
                route: "/User/SetUserFunction",
                remark: "设置用户的功能",
            },
        ]
    }
}
