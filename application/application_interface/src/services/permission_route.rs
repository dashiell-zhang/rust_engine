use application_model::authorize::models::PermissionRouteSyncItem;
use async_trait::async_trait;
use common::app_error::AppError;

/// 宿主权限路由同步应用契约。
#[async_trait]
pub trait PermissionRouteService: Send + Sync {
    async fn sync_routes(
        &self,
        module_name: &str,
        routes: &[PermissionRouteSyncItem<'_>],
    ) -> Result<bool, AppError>;
}
