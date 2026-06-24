use crate::{
    features::protect_method_router,
    handlers::authorize::{generate_ecdsa_key_pair, get_function_list, get_token, update_route},
    openapi::authorize::AuthorizeApiDoc,
    state::AppState,
};
use axum::Router;
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};

use super::{WebApiFeature, WebApiRoutePermission};

/// 授权特性
pub struct AuthorizeFeature;

impl WebApiFeature for AuthorizeFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route("/Authorize/GetToken", axum::routing::post(get_token))
            .route(
                "/Authorize/GetFunctionList",
                protect_method_router(
                    axum::routing::get(get_function_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Authorize/GetFunctionList",
                    false,
                ),
            )
            .route("/Authorize/UpdateRoute", axum::routing::get(update_route))
            .route(
                "/Authorize/GenerateEcdsaKeyPair",
                axum::routing::get(generate_ecdsa_key_pair),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(AuthorizeApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![
            WebApiRoutePermission {
                route: "/Authorize/GetFunctionList",
                remark: "获取当前用户可访问的功能列表",
            },
            WebApiRoutePermission {
                route: "/Authorize/UpdateRoute",
                remark: "更新路由信息表",
            },
        ]
    }
}
