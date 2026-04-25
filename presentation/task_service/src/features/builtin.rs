use task_service_core::features::{TaskServiceFeature, TaskServiceFeatureContext};
use task_service_core::registry::TaskRegistry;

use crate::tasks::register_builtin_tasks;

/// 内置任务特性
pub struct BuiltinTaskFeature;

impl TaskServiceFeature for BuiltinTaskFeature {
    /// 向任务注册表注册当前特性的任务
    fn register(&self, registry: &mut TaskRegistry, context: &TaskServiceFeatureContext) {
        register_builtin_tasks(registry, context);
    }
}
