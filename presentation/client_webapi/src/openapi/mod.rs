use utoipa::OpenApi;
use webapi_core::openapi::BearerAuth;

/// 授权文档
pub mod authorize;
/// 基础工具文档
pub mod base;

/// 服务与 LLM 演示文档。
pub mod demo;

/// 文件文档
pub mod file;

/// 支付文档
pub mod pay;

/// 用户管理文档
pub mod user;

/// 客户端 OpenAPI 文档定义
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Client WebAPI",
        version = "1.0.0",
        description = "客户端 WebAPI 服务"
    ),
    modifiers(&BearerAuth)
)]
pub struct ApiDoc;
