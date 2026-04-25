/// Bearer JWT 安全方案注入器
pub use crate::openapi::BearerAuth;

use utoipa::OpenApi;

/// WebAPI 基础 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    info(
        title = "WebAPI",
        version = "1.0.0",
        description = "基于 Axum + SeaORM 的 WebAPI 服务"
    ),
    modifiers(&BearerAuth)
)]
pub struct ApiDoc;
