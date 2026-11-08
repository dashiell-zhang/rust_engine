use crate::openapi::file::FileApiDoc;
use axum::extract::DefaultBodyLimit;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

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

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::file::upload_file).configure(
                |route, _state| {
                    route.map(|route| {
                        route.layer(DefaultBodyLimit::disable()).route_layer(
                            axum::middleware::from_fn(
                                webapi_core::uploaded_file::require_multipart,
                            ),
                        )
                    })
                },
            ),
            webapi_core::endpoint!(crate::handlers::file::get_file_url),
            webapi_core::endpoint!(crate::handlers::file::get_image),
            webapi_core::endpoint!(crate::handlers::file::get_file),
            webapi_core::endpoint!(crate::handlers::file::delete_file),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(FileApiDoc::openapi())
    }
}
