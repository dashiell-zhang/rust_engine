/// 客户端业务特性定义
pub mod features;

/// 客户端请求处理器
pub mod handlers;

/// 客户端 OpenAPI 文档
pub mod openapi;

/// 客户端宿主装配
pub mod host_support;

/// 客户端 LLM 宿主适配
pub mod llm_support;

/// 复用共享宿主状态定义
pub use webapi_core::state;

use webapi_core::host::{WebApiHostProfile, run_webapi_host};

/// 客户端 WebAPI 宿主档案
pub const CLIENT_WEBAPI_PROFILE: WebApiHostProfile = WebApiHostProfile {
    host_name: "client_webapi",
    default_config_path: "presentation/client_webapi/appsettings",
    permission_module: "client_webapi",
    default_lock_prefix: "client_webapi",
    default_logger_project: "client_webapi",
    default_file_root: "presentation/client_webapi/storage",
    build_distributed_lock: host_support::build_distributed_lock,
    build_cache: host_support::build_cache,
    build_logger_runtime: host_support::build_logger_runtime,
    build_file_storage: host_support::build_file_storage,
    build_email_sender: host_support::build_email_sender,
};

/// 启动客户端 WebAPI 宿主
pub async fn run() -> Result<(), String> {
    run_webapi_host(&CLIENT_WEBAPI_PROFILE, features::features()).await
}
