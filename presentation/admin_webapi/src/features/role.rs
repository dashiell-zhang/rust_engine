use axum::Router;
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::queue_limit::{QueueLimitOptions, queue_limit_method_router};

use crate::{
    features::protect_method_router,
    handlers::role::{
        create_role, delete_role, get_role, get_role_function, get_role_list, get_role_select,
        set_role_function, update_role,
    },
    openapi::role::RoleApiDoc,
    state::AppState,
};

use super::{WebApiFeature, WebApiRoutePermission};

/// 角色管理特性
pub struct RoleFeature;

impl WebApiFeature for RoleFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/Role/GetRoleList",
                protect_method_router(
                    axum::routing::get(get_role_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Role/GetRoleList",
                    false,
                ),
            )
            .route(
                "/Role/GetRole",
                protect_method_router(
                    axum::routing::get(get_role),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Role/GetRole",
                    false,
                ),
            )
            .route(
                "/Role/CreateRole",
                protect_method_router(
                    queue_limit_method_router(
                        axum::routing::post(create_role),
                        app_state.role_code_lock.clone(),
                        QueueLimitOptions::rejecting_with_authorization(1),
                    ),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Role/CreateRole",
                    false,
                ),
            )
            .route(
                "/Role/UpdateRole",
                protect_method_router(
                    queue_limit_method_router(
                        axum::routing::post(update_role),
                        app_state.role_code_lock.clone(),
                        QueueLimitOptions::rejecting_with_authorization(1),
                    ),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Role/UpdateRole",
                    false,
                ),
            )
            .route(
                "/Role/DeleteRole",
                protect_method_router(
                    axum::routing::delete(delete_role),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Role/DeleteRole",
                    false,
                ),
            )
            .route(
                "/Role/GetRoleFunction",
                protect_method_router(
                    axum::routing::get(get_role_function),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Role/GetRoleFunction",
                    false,
                ),
            )
            .route(
                "/Role/SetRoleFunction",
                protect_method_router(
                    axum::routing::post(set_role_function),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Role/SetRoleFunction",
                    false,
                ),
            )
            .route(
                "/Role/GetRoleSelect",
                protect_method_router(
                    axum::routing::get(get_role_select),
                    app_state,
                    permission_module,
                    "/Role/GetRoleSelect",
                    false,
                ),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(RoleApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![
            WebApiRoutePermission {
                route: "/Role/GetRoleList",
                remark: "获取角色列表",
            },
            WebApiRoutePermission {
                route: "/Role/GetRole",
                remark: "获取单个角色信息",
            },
            WebApiRoutePermission {
                route: "/Role/CreateRole",
                remark: "创建角色",
            },
            WebApiRoutePermission {
                route: "/Role/UpdateRole",
                remark: "更新角色",
            },
            WebApiRoutePermission {
                route: "/Role/DeleteRole",
                remark: "删除角色",
            },
            WebApiRoutePermission {
                route: "/Role/GetRoleFunction",
                remark: "获取角色功能树",
            },
            WebApiRoutePermission {
                route: "/Role/SetRoleFunction",
                remark: "设置角色功能勾选状态",
            },
            WebApiRoutePermission {
                route: "/Role/GetRoleSelect",
                remark: "获取角色下拉列表",
            },
        ]
    }
}
