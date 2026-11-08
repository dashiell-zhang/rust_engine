use task_service_core::features::TaskServiceFeature;

/// 内置任务特性
pub mod builtin;

/// 获取默认特性列表
pub fn features() -> Vec<Box<dyn TaskServiceFeature>> {
    vec![Box::new(builtin::BuiltinTaskFeature {
        services: crate::service_factory::business_services,
    })]
}
