use crate::{
    features::protect_method_router,
    handlers::file::{delete_file, get_file, get_file_url, get_image, upload_file},
    openapi::file::FileApiDoc,
    state::AppState,
};
use axum::extract::DefaultBodyLimit;
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;

use super::{WebApiFeature, WebApiRoutePermission};

/// 文件管理特性
pub struct FileFeature;

impl WebApiFeature for FileFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::File>(resources)?;
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
                "/File/UploadFile",
                protect_method_router(
                    webapi_core::routes::post(upload_file),
                    app_state.clone(),
                    permission_module.clone(),
                    "/File/UploadFile",
                    false,
                )
                .map(|route| {
                    route
                        .layer(DefaultBodyLimit::disable())
                        .route_layer(axum::middleware::from_fn(
                            webapi_core::uploaded_file::require_multipart,
                        ))
                }),
            )
            .route(
                "/File/GetFileUrl",
                protect_method_router(
                    webapi_core::routes::get(get_file_url),
                    app_state.clone(),
                    permission_module.clone(),
                    "/File/GetFileUrl",
                    false,
                ),
            )
            .route(
                "/File/GetImage",
                webapi_core::sign_verify::sign_verify_method_router(
                    webapi_core::routes::get(get_image),
                    app_state.clone(),
                ),
            )
            .route(
                "/File/GetFile",
                webapi_core::sign_verify::sign_verify_method_router(
                    webapi_core::routes::get(get_file),
                    app_state.clone(),
                ),
            )
            .route(
                "/File/DeleteFile",
                protect_method_router(
                    webapi_core::routes::delete(delete_file),
                    app_state,
                    permission_module,
                    "/File/DeleteFile",
                    false,
                ),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(FileApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![
            WebApiRoutePermission {
                route: "/File/UploadFile",
                remark: "单文件上传接口",
            },
            WebApiRoutePermission {
                route: "/File/GetFileUrl",
                remark: "通过文件ID获取文件静态访问路径",
            },
            WebApiRoutePermission {
                route: "/File/DeleteFile",
                remark: "通过文件ID删除文件方法",
            },
        ]
    }
}
