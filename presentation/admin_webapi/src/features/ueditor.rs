use crate::{handlers::ueditor::process_request, state::AppState};
use axum::extract::DefaultBodyLimit;
use std::sync::Arc;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::builder::protect_method_router;
use webapi_core::routes::WebApiRoutes as Router;

use super::{WebApiFeature, WebApiRoutePermission};

/// 对接 UEditor 前端协议的管理端特性
/// UEditor 富文本上传特性
pub struct UEditorFeature;

impl WebApiFeature for UEditorFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::File>(resources)?;
        Ok(())
    }
    /// 注册 UEditor 协议路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router.route(
            "/Ueditor/ProcessRequest",
            protect_method_router(
                webapi_core::routes::get(process_request).post(process_request),
                app_state,
                permission_module,
                "/Ueditor/ProcessRequest",
                false,
            )
            .map(|route| route.layer(DefaultBodyLimit::disable())),
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
            remark: "处理 UEditor 配置、上传和远程图片抓取请求",
        }]
    }
}
