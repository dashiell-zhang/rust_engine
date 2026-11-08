use crate::registry::TaskRegistry;
use common::identifier_generator::IdentifierGenerator;
use std::sync::Arc;

/// TaskService 特性注册上下文
#[derive(Clone)]
pub struct TaskServiceFeatureContext {
    /// 宿主业务缓存；任务特性按需克隆使用，不参与 ID 节点协调。
    pub cache: Arc<dyn distributed_cache::DistributedCache>,
    pub event_publisher: event_bus::EventPublisher,
    /// 业务特性按模块类型取得固定目标。
    pub modules: Arc<repository_core::module::DatabaseModules>,

    /// ID 生成器
    pub identifier_generator: IdentifierGenerator,

    /// 任务入口按需注册的资源；公共核心不要求消息或文件组件。
    pub resources: Arc<common::resources::Resources>,
}

/// TaskService 业务特性
pub trait TaskServiceFeature: Send + Sync {
    fn register_event_listeners(
        &self,
        _registry: &mut event_bus::EventListenerRegistry,
        _context: &TaskServiceFeatureContext,
    ) -> Result<(), String> {
        Ok(())
    }

    /// 向任务注册表注册当前特性的任务
    fn register(
        &self,
        registry: &mut TaskRegistry,
        context: &TaskServiceFeatureContext,
    ) -> Result<(), String>;
}

impl event_bus::EventPublisherProvider for TaskServiceFeatureContext {
    fn event_publisher(&self) -> &event_bus::EventPublisher {
        &self.event_publisher
    }
}
