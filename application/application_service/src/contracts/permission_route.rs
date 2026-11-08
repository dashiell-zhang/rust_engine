use application_model::authorize::models::PermissionRouteSyncItem;
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::permission_route::PermissionRouteService
    for crate::permission_route::service::PermissionRouteService<'_>
{
    async fn sync_routes(
        &self,
        module_name: &str,
        routes: &[PermissionRouteSyncItem<'_>],
    ) -> Result<bool, AppError> {
        crate::permission_route::service::PermissionRouteService::sync_routes(
            self,
            module_name,
            routes,
        )
        .await
    }
}
