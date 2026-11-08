/// TaskService 业务特性定义
pub mod features;
pub mod service_factory;

/// TaskService 内置任务实现
pub mod tasks;

/// TaskService 宿主装配
pub mod host_support;

use task_service_core::{
    builder::TaskServiceHostBuilder,
    settings::{Settings, TaskServiceHostProfile},
};

/// TaskService 宿主档案
pub const TASK_SERVICE_PROFILE: TaskServiceHostProfile = TaskServiceHostProfile {
    build_databases: host_support::build_databases,
    host_name: "task_service",
    default_config_path: "presentation/task_service/appsettings",
    default_lock_prefix: "task_service",
    default_logger_project: "task_service",
    default_file_root: "presentation/task_service/storage",
    build_distributed_lock: host_support::build_distributed_lock,
    build_distributed_cache: host_support::build_distributed_cache,
    build_logger_runtime: host_support::build_logger_runtime,
    build_event_transport: host_support::build_event_transport,
    build_sms_sender: host_support::build_sms_sender,
    build_email_sender: host_support::build_email_sender,
    build_file_storage: host_support::build_file_storage,
};

/// 启动任务宿主
pub async fn run() -> Result<(), String> {
    let settings = Settings::load(&TASK_SERVICE_PROFILE);
    let host = TaskServiceHostBuilder::new(
        settings,
        TASK_SERVICE_PROFILE.build_distributed_lock,
        TASK_SERVICE_PROFILE.build_distributed_cache,
        TASK_SERVICE_PROFILE.build_logger_runtime,
        TASK_SERVICE_PROFILE.build_sms_sender,
        TASK_SERVICE_PROFILE.build_email_sender,
        TASK_SERVICE_PROFILE.build_file_storage,
    )
    .with_databases_factory(TASK_SERVICE_PROFILE.build_databases)
    .with_event_transport_factory(TASK_SERVICE_PROFILE.build_event_transport)
    .with_features(features::features())
    .build()
    .await?;

    host.run().await?;
    Ok(())
}
