use axum::extract::DefaultBodyLimit;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

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

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::ueditor::process_request).configure(
                |route, _state| route.map(|route| route.layer(DefaultBodyLimit::disable())),
            ),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        None
    }
}
