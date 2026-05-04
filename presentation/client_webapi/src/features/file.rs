use axum::{Router, extract::DefaultBodyLimit};
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::builder::protect_method_router;

use crate::{
    handlers::file::{
        delete_file, get_file, get_file_url, get_image, remote_upload_file, upload_file,
    },
    openapi::file::FileApiDoc,
    state::AppState,
};

use super::{WebApiFeature, WebApiRoutePermission};

/// 文件管理特性
pub struct FileFeature;

impl WebApiFeature for FileFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();
        let max_upload_bytes = app_state.settings.file.max_upload_bytes;

        router
            .route(
                "/File/UploadFile",
                protect_method_router(
                    axum::routing::post(upload_file),
                    app_state.clone(),
                    permission_module.clone(),
                    "/File/UploadFile",
                    false,
                )
                .layer(DefaultBodyLimit::max(max_upload_bytes)),
            )
            .route(
                "/File/RemoteUploadFile",
                protect_method_router(
                    axum::routing::post(remote_upload_file),
                    app_state.clone(),
                    permission_module.clone(),
                    "/File/RemoteUploadFile",
                    false,
                ),
            )
            .route(
                "/File/GetFileUrl",
                protect_method_router(
                    axum::routing::get(get_file_url),
                    app_state.clone(),
                    permission_module.clone(),
                    "/File/GetFileUrl",
                    false,
                ),
            )
            .route("/File/GetImage", axum::routing::get(get_image))
            .route("/File/GetFile", axum::routing::get(get_file))
            .route(
                "/File/DeleteFile",
                protect_method_router(
                    axum::routing::delete(delete_file),
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
                remark: "上传文件",
            },
            WebApiRoutePermission {
                route: "/File/RemoteUploadFile",
                remark: "远程上传文件",
            },
            WebApiRoutePermission {
                route: "/File/GetFileUrl",
                remark: "获取文件访问地址",
            },
            WebApiRoutePermission {
                route: "/File/DeleteFile",
                remark: "删除文件",
            },
        ]
    }
}
