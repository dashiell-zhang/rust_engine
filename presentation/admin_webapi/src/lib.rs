pub mod business_resources;
/// 管理端业务特性定义
pub mod features;

/// 管理端请求处理器
pub mod handlers;

/// 管理端 OpenAPI 文档
pub mod openapi;

/// 管理端 LLM 宿主适配
pub mod llm_support;

/// 复用共享宿主状态定义
pub use webapi_core::state;

use webapi_core::host::{WebApiHostProfile, run_webapi_host};

/// 管理端 WebAPI 宿主档案
pub const ADMIN_WEBAPI_PROFILE: WebApiHostProfile = WebApiHostProfile {
    configure_http_clients: None,
    application: webapi_support::application,
    build_resources: business_resources::build,
    host_name: "admin_webapi",
    default_config_path: "presentation/admin_webapi/appsettings",
    permission_module: "Admin.WebAPI",
    default_lock_prefix: "admin_webapi",
    default_logger_project: "admin_webapi",
    default_file_root: "wwwroot",
    check_log_read_health: true,
    serve_web_root: true,
    build_distributed_lock: llm_support::build_distributed_lock,
    build_cache: llm_support::build_cache,
    build_logger_runtime: llm_support::build_logger_runtime,
    build_event_transport: llm_support::build_event_transport,
};

/// 启动管理端 WebAPI 宿主
pub async fn run() -> Result<(), String> {
    run_webapi_host(&ADMIN_WEBAPI_PROFILE, features::features()).await
}
