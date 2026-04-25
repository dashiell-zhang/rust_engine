use crate::{features::protect_method_router, handlers::ueditor::process_request, state::AppState};
use axum::Router;
use std::sync::Arc;
use utoipa::openapi::OpenApi as OpenApiDocument;

use super::{WebApiFeature, WebApiRoutePermission};

/// 对接 UEditor 前端协议的管理端特性
/// UEditor 富文本上传特性
pub struct UEditorFeature;

impl WebApiFeature for UEditorFeature {
    /// 注册 UEditor 协议路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        // 兼容历史前端对首段路径使用小写 ueditor 的调用方式
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/Ueditor/ProcessRequest",
                protect_method_router(
                    axum::routing::get(process_request).post(process_request),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Ueditor/ProcessRequest",
                    false,
                ),
            )
            .route(
                "/ueditor/ProcessRequest",
                protect_method_router(
                    axum::routing::get(process_request).post(process_request),
                    app_state,
                    permission_module,
                    "/Ueditor/ProcessRequest",
                    false,
                ),
            )
    }

    /// 当前特性暂不提供独立 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        None
    }

    /// 获取 UEditor 路由权限描述
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![WebApiRoutePermission {
            route: "/Ueditor/ProcessRequest",
            remark: "处理 UEditor 请求",
        }]
    }
}
