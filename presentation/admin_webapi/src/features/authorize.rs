use crate::{
    features::protect_method_router,
    handlers::authorize::{
        generate_ecdsa_key_pair, get_function_list, get_token, init_data, update_route,
    },
    openapi::authorize::AuthorizeApiDoc,
    state::AppState,
};
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;

use super::{WebApiFeature, WebApiRoutePermission};

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
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/Authorize/GetToken",
                webapi_core::routes::post(get_token).json_body("login"),
            )
            .route("/Authorize/InitData", webapi_core::routes::post(init_data))
            .route(
                "/Authorize/GetFunctionList",
                protect_method_router(
                    webapi_core::routes::get(get_function_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Authorize/GetFunctionList",
                    false,
                ),
            )
            .route(
                "/Authorize/UpdateRoute",
                webapi_core::routes::get(update_route),
            )
            .route(
                "/Authorize/GenerateEcdsaKeyPair",
                webapi_core::routes::get(generate_ecdsa_key_pair),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(AuthorizeApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![WebApiRoutePermission {
            route: "/Authorize/GetFunctionList",
            remark: "获取授权功能列表",
        }]
    }
}
