//! 绑定到宿主选定数据库的通用任务实体，不依赖业务用户模型。
pub mod management;
pub mod producer;
pub mod queue_task;
pub mod store;
pub mod task_setting;

pub use producer::{NewQueueTask, QueueTaskProducer};
pub use store::{DatabaseQueueTaskStore, QueueTaskStore, TaskSettingDefinition};

/// 带参定时任务模板参数占位值。
pub const ARGS_DEFAULT_PARAMETER: &str = "__args_default__";

pub use management::{
    NewScheduleTask, QueueTaskManagement, QueueTaskQuery, TaskPage, TaskSettingChange,
    TaskSettingUpdate,
};
repository_core::impl_json_index_model!(queue_task::Entity, task_setting::Entity);

/// 当前模型迁移元数据。
pub mod schema;
