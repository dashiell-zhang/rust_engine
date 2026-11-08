//! 运行入口注册的资源与健康检查，公共宿主不选择业务模型。
use crate::health::HealthComponentResponse;
use async_trait::async_trait;
use common::resources::Resources;
use std::sync::Arc;

#[async_trait]
pub trait ResourceHealthCheck: Send + Sync {
    fn name(&self) -> &str;
    async fn check(&self, stopping: &event_bus::CancellationToken) -> HealthComponentResponse;
}

#[derive(Default)]
pub struct WebApiResources {
    values: Resources,
    health: Vec<Arc<dyn ResourceHealthCheck>>,
}
impl WebApiResources {
    /// 同一资源类型只能注册一次，禁止按顺序覆盖已有绑定。
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Result<(), String> {
        self.values.insert(value)
    }
    pub fn get<T: Send + Sync + 'static>(&self) -> Result<&T, String> {
        self.values.get()
    }
    pub fn add_health_check(&mut self, check: Arc<dyn ResourceHealthCheck>) -> Result<(), String> {
        if self.health.iter().any(|item| item.name() == check.name()) {
            return Err(format!("健康检查重复注册: {}", check.name()));
        }
        self.health.push(check);
        Ok(())
    }
    pub async fn check_health(
        &self,
        stopping: &event_bus::CancellationToken,
    ) -> Vec<HealthComponentResponse> {
        futures_util::future::join_all(self.health.iter().map(|check| check.check(stopping))).await
    }
}
pub use common::resource_cleanup::ResourceCleanup;
