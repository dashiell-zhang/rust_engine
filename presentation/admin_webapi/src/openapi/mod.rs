use utoipa::OpenApi;
use webapi_core::openapi::BearerAuth;

/// 基础工具文档
pub mod base;

/// 文章管理文档
pub mod article;

/// 授权文档
pub mod authorize;

/// 友情链接文档
pub mod link;

/// 文件文档
pub mod file;

/// LLM 文档
pub mod llm;

/// 日志管理文档
pub mod log;

/// 角色管理文档
pub mod role;

/// 任务中心文档
pub mod task;

/// 用户管理文档
pub mod user;

/// 站点管理文档
pub mod site;

/// 管理端 OpenAPI 文档定义
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Admin WebAPI",
        version = "1.0.0",
        description = "管理端 WebAPI 服务"
    ),
    modifiers(&BearerAuth)
)]
pub struct ApiDoc;
