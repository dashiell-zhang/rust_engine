use crate::{
    features::protect_method_router,
    handlers::link::{create_link, delete_link, get_link, get_link_list, update_link},
    openapi::link::LinkApiDoc,
    state::AppState,
};
use axum::Router;
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};

use super::{WebApiFeature, WebApiRoutePermission};

/// 友情链接管理特性
pub struct LinkFeature;

impl WebApiFeature for LinkFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/Site/GetLinkList",
                protect_method_router(
                    axum::routing::get(get_link_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Site/GetLinkList",
                    false,
                ),
            )
            .route(
                "/Site/GetLink",
                protect_method_router(
                    axum::routing::get(get_link),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Site/GetLink",
                    false,
                ),
            )
            .route(
                "/Site/CreateLink",
                protect_method_router(
                    axum::routing::post(create_link),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Site/CreateLink",
                    false,
                ),
            )
            .route(
                "/Site/UpdateLink",
                protect_method_router(
                    axum::routing::post(update_link),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Site/UpdateLink",
                    false,
                ),
            )
            .route(
                "/Site/DeleteLink",
                protect_method_router(
                    axum::routing::delete(delete_link),
                    app_state,
                    permission_module,
                    "/Site/DeleteLink",
                    false,
                ),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(LinkApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![
            WebApiRoutePermission {
                route: "/Site/GetLinkList",
                remark: "获取友情链接列表",
            },
            WebApiRoutePermission {
                route: "/Site/GetLink",
                remark: "获取单个友情链接",
            },
            WebApiRoutePermission {
                route: "/Site/CreateLink",
                remark: "创建友情链接",
            },
            WebApiRoutePermission {
                route: "/Site/UpdateLink",
                remark: "更新友情链接",
            },
            WebApiRoutePermission {
                route: "/Site/DeleteLink",
                remark: "删除友情链接",
            },
        ]
    }
}
