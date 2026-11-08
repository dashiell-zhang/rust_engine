use crate::{
    features::protect_method_router,
    handlers::site::{get_server_info, get_site, update_site},
    openapi::site::SiteApiDoc,
    state::AppState,
};
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;

use super::{WebApiFeature, WebApiRoutePermission};

/// 站点管理特性
pub struct SiteFeature;

impl WebApiFeature for SiteFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        // 三个站点接口都挂在同一权限模块下 便于后台统一授权
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/Site/GetServerInfo",
                protect_method_router(
                    webapi_core::routes::get(get_server_info),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Site/GetServerInfo",
                    false,
                ),
            )
            .route(
                "/Site/GetSite",
                protect_method_router(
                    webapi_core::routes::get(get_site),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Site/GetSite",
                    false,
                ),
            )
            .route(
                "/Site/EditSite",
                protect_method_router(
                    webapi_core::routes::post(update_site).json_body("editSite"),
                    app_state,
                    permission_module,
                    "/Site/EditSite",
                    false,
                ),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(SiteApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        // 返回静态路由清单 供初始化权限种子和路由绑定使用
        vec![
            WebApiRoutePermission {
                route: "/Site/GetServerInfo",
                remark: "获取服务器信息",
            },
            WebApiRoutePermission {
                route: "/Site/GetSite",
                remark: "获取站点信息",
            },
            WebApiRoutePermission {
                route: "/Site/EditSite",
                remark: "编辑站点信息",
            },
        ]
    }
}
