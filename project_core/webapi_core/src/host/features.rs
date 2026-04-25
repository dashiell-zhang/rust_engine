use axum::Router;
use std::sync::Arc;
use utoipa::openapi::OpenApi;

use crate::host::state::AppState;

/// WebAPI 路由权限注册信息
#[derive(Clone, Copy)]
pub struct WebApiRoutePermission {
    /// 路由路径
    pub route: &'static str,

    /// 路由说明
    pub remark: &'static str,
}

/// WebAPI 业务特性
pub trait WebApiFeature: Send + Sync {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>>;

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApi>;

    /// 获取当前特性需要同步的权限路由
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![]
    }
}
