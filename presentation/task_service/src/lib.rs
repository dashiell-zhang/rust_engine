/// TaskService 业务特性定义
pub mod features;
pub mod service_factory;

/// TaskService 内置任务实现
pub mod tasks;

pub mod configuration;
/// TaskService 宿主装配
pub mod host_support;

use task_service_core::{
    builder::TaskServiceHostBuilder,
    settings::{Settings, TaskServiceHostProfile},
};

/// TaskService 宿主档案
pub const TASK_SERVICE_PROFILE: TaskServiceHostProfile = TaskServiceHostProfile {
    validate_configuration: Some(configuration::validate),
    build_databases: host_support::build_databases,
    host_name: "task_service",
    default_config_path: "presentation/task_service/appsettings",
    default_lock_prefix: "task_service",
    default_logger_project: "task_service",
    build_distributed_lock: host_support::build_distributed_lock,
    build_distributed_cache: host_support::build_distributed_cache,
    build_logger_runtime: host_support::build_logger_runtime,
    build_event_transport: host_support::build_event_transport,
    build_resources: Some(host_support::build_resources),
};

/// 启动任务宿主
pub async fn run() -> Result<(), String> {
    let settings = Settings::load(&TASK_SERVICE_PROFILE);
    let host = TaskServiceHostBuilder::new(
        settings,
        TASK_SERVICE_PROFILE.build_distributed_lock,
        TASK_SERVICE_PROFILE.build_distributed_cache,
        TASK_SERVICE_PROFILE.build_logger_runtime,
    )
    .with_resources_factory(TASK_SERVICE_PROFILE.build_resources)
    .with_databases_factory(TASK_SERVICE_PROFILE.build_databases)
    .with_event_transport_factory(TASK_SERVICE_PROFILE.build_event_transport)
    .with_features(features::features())
    .build()
    .await?;

    host.run().await?;
    Ok(())
}
