use crate::{
    features::protect_method_router, handlers::log::get_log_list, openapi::log::LogApiDoc,
    state::AppState,
};
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;

use super::{WebApiFeature, WebApiRoutePermission};

/// 日志管理特性
pub struct LogFeature;

impl WebApiFeature for LogFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::LogManage>(resources)?;
        Ok(())
    }
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router.route(
            "/Operations/GetLogList",
            protect_method_router(
                webapi_core::routes::get(get_log_list),
                app_state,
                permission_module,
                "/Operations/GetLogList",
                false,
            ),
        )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(LogApiDoc::openapi())
    }

    /// 获取当前特性需要同步的权限路由
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![WebApiRoutePermission {
            route: "/Operations/GetLogList",
            remark: "获取日志列表",
        }]
    }
}
