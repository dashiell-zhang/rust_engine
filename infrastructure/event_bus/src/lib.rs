//! 跨进程广播；每个实例独立消费，不提供业务事务原子性或重启补收。
mod runtime;
pub use runtime::{EventRuntime, PreparedEventBus};
pub use tokio_util::sync::CancellationToken;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::FutureExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{any::TypeId, collections::HashMap, future::Future, pin::Pin, sync::Arc};
use uuid::Uuid;

pub type EventResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventProvider {
    #[default]
    Redis,
    Database,
}

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct EventBusOptions {
    pub provider: EventProvider,
    pub scope: String,
    /// 事件专用目标；宿主在为空时明确选择业务 Redis 或数据库地址。
    pub connection: String,
    pub retention_milliseconds: u64,
    pub poll_interval_milliseconds: u64,
    pub retry_interval_milliseconds: u64,
    pub cleanup_interval_milliseconds: u64,
    pub batch_size: usize,
}
impl Default for EventBusOptions {
    fn default() -> Self {
        Self {
            provider: EventProvider::Redis,
            scope: "NetEngine".into(),
            connection: String::new(),
            retention_milliseconds: 3_600_000,
            poll_interval_milliseconds: 1000,
            retry_interval_milliseconds: 1000,
            cleanup_interval_milliseconds: 60_000,
            batch_size: 100,
        }
    }
}
impl EventBusOptions {
    pub fn validate(&self) -> EventResult<()> {
        if self.scope.trim().is_empty() || self.scope.encode_utf16().count() > 200 {
            return Err("事件 Scope 必须为 1～200 个字符".into());
        }
        // 与源 TimeSpan 的正数范围一致；配置以完整毫秒表示。
        if self.retention_milliseconds == 0 || self.retention_milliseconds > 922_337_203_685_477 {
            return Err("事件保留时间超出有效范围".into());
        }
        if !(1..=1000).contains(&self.batch_size) {
            return Err("事件批量大小必须介于 1 和 1000".into());
        }
        for interval in [
            self.poll_interval_milliseconds,
            self.retry_interval_milliseconds,
            self.cleanup_interval_milliseconds,
        ] {
            if !(1..=4_294_967_294).contains(&interval) {
                return Err("事件间隔必须介于 1 和 4294967294 毫秒".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct EventEnvelope {
    pub event_id: Uuid,
    pub event_name: String,
    pub payload: String,
}
#[derive(Clone, Debug)]
pub struct EventRecord {
    pub position: String,
    pub published_at: DateTime<Utc>,
    pub message: Option<EventEnvelope>,
}

#[async_trait]
pub trait EventTransport: Send + Sync {
    async fn position(&self) -> EventResult<String>;
    async fn append(&self, message: EventEnvelope) -> EventResult<()>;
    async fn read(&self, position: &str, count: usize) -> EventResult<Vec<EventRecord>>;
    async fn trim(&self, before: DateTime<Utc>) -> EventResult<()>;
    /// 连接检查不得发布事件或改变游标。
    async fn health(&self) -> EventResult<()>;

    async fn health_with_cancellation(&self, stopping: &CancellationToken) -> EventResult<()> {
        tokio::select! { biased; _ = stopping.cancelled() => Err("事件操作已取消".into()), result = self.health() => result }
    }

    /// 后端可在取消时等待服务器端清理；默认实现适用于无独立查询取消协议的后端。
    async fn append_with_cancellation(
        &self,
        message: EventEnvelope,
        stopping: &CancellationToken,
    ) -> EventResult<()> {
        tokio::select! { biased; _ = stopping.cancelled() => Err("事件操作已取消".into()), result = self.append(message) => result }
    }
    async fn position_with_cancellation(
        &self,
        stopping: &CancellationToken,
    ) -> EventResult<String> {
        tokio::select! { biased; _ = stopping.cancelled() => Err("事件操作已取消".into()), result = self.position() => result }
    }
    async fn read_with_cancellation(
        &self,
        position: &str,
        count: usize,
        stopping: &CancellationToken,
    ) -> EventResult<Vec<EventRecord>> {
        tokio::select! { biased; _ = stopping.cancelled() => Err("事件操作已取消".into()), result = self.read(position, count) => result }
    }
    async fn trim_with_cancellation(
        &self,
        before: DateTime<Utc>,
        stopping: &CancellationToken,
    ) -> EventResult<()> {
        tokio::select! { biased; _ = stopping.cancelled() => Err("事件操作已取消".into()), result = self.trim(before) => result }
    }
}

/// 显式稳定名称可与其他语言的完整事件类型名一致；禁止以 Rust 内部路径作为隐式协议。
pub trait Event: Serialize + DeserializeOwned + Send + Sync + 'static {
    const NAME: &'static str;
}

#[derive(Clone)]
pub struct EventPublisher {
    transport: Arc<dyn EventTransport>,
}
impl EventPublisher {
    pub fn new(transport: Arc<dyn EventTransport>) -> Self {
        Self { transport }
    }
    pub async fn publish<E: Event>(&self, message: &E) -> EventResult<()> {
        self.publish_with_cancellation(message, &CancellationToken::new())
            .await
    }
    /// 取消后仍需等待该 future 返回，让数据库后端完成在途事务的清理。
    pub async fn publish_with_cancellation<E: Event>(
        &self,
        message: &E,
        stopping: &CancellationToken,
    ) -> EventResult<()> {
        if stopping.is_cancelled() {
            return Err("事件操作已取消".into());
        }
        validate_name(E::NAME)?;
        self.transport
            .append_with_cancellation(
                EventEnvelope {
                    event_id: Uuid::new_v4(),
                    event_name: E::NAME.into(),
                    payload: common::json_helper::web_object_to_json(message)
                        .map_err(|error| error.to_string())?,
                },
                stopping,
            )
            .await
    }
    pub async fn health(&self) -> EventResult<()> {
        self.transport.health().await
    }
    pub async fn health_with_cancellation(&self, stopping: &CancellationToken) -> EventResult<()> {
        self.transport.health_with_cancellation(stopping).await
    }
}

pub fn validate_name(name: &str) -> EventResult<()> {
    if name.trim().is_empty() {
        Err("事件名称不能为空".into())
    } else {
        Ok(())
    }
}

type HandlerFuture = Pin<Box<dyn Future<Output = EventResult<()>> + Send>>;
type Handler = dyn Fn(String, CancellationToken) -> HandlerFuture + Send + Sync;

/// 每次监听调用（包括重试）独立创建的处理器；异步资源由 close 释放。
#[async_trait]
pub trait EventHandler<E: Event>: Send {
    async fn handle(&mut self, event: E, stopping: CancellationToken) -> EventResult<()>;

    /// 参数解析、处理成功或失败后均调用；不另起释放任务，遵守宿主停机预算。
    async fn close(&mut self) -> EventResult<()> {
        Ok(())
    }
}

struct CallbackHandler<F>(Arc<F>);
#[async_trait]
impl<E, F, Fut> EventHandler<E> for CallbackHandler<F>
where
    E: Event,
    F: Fn(E, CancellationToken) -> Fut + Send + Sync,
    Fut: Future<Output = EventResult<()>> + Send,
{
    async fn handle(&mut self, event: E, stopping: CancellationToken) -> EventResult<()> {
        (self.0)(event, stopping).await
    }
}

pub(crate) struct Listener {
    pub identity: String,
    pub invoke: Arc<Handler>,
}
#[derive(Default)]
pub struct EventListenerRegistry {
    pub(crate) listeners: HashMap<String, Vec<Listener>>,
    event_types: HashMap<String, TypeId>,
}
impl EventListenerRegistry {
    /// 普通闭包监听；需要统一异步释放的独立服务使用 subscribe_factory。
    pub fn subscribe<E, F, Fut>(&mut self, identity: &str, handler: F) -> EventResult<()>
    where
        E: Event,
        F: Fn(E, CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = EventResult<()>> + Send + 'static,
    {
        let handler = Arc::new(handler);
        self.subscribe_factory::<E, _, _>(identity, move || Ok(CallbackHandler(handler.clone())))
    }

    /// 工厂先创建本次作用域，再解析事件；每次重试重新创建，调用结束后等待 close。
    pub fn subscribe_factory<E, F, H>(&mut self, identity: &str, factory: F) -> EventResult<()>
    where
        E: Event,
        F: Fn() -> EventResult<H> + Send + Sync + 'static,
        H: EventHandler<E> + 'static,
    {
        validate_name(E::NAME)?;
        if identity.trim().is_empty() {
            return Err("监听标识不能为空".into());
        }
        if self
            .event_types
            .get(E::NAME)
            .is_some_and(|existing| *existing != TypeId::of::<E>())
        {
            return Err(format!("事件名称冲突: {}", E::NAME));
        }
        let listeners = self.listeners.entry(E::NAME.into()).or_default();
        if listeners
            .iter()
            .any(|listener| listener.identity == identity)
        {
            return Err(format!("监听重复注册: {identity}"));
        }
        self.event_types.insert(E::NAME.into(), TypeId::of::<E>());
        let factory = Arc::new(factory);
        listeners.push(Listener {
            identity: identity.into(),
            invoke: Arc::new(move |payload, token| {
                let factory = factory.clone();
                Box::pin(async move {
                    let mut handler = factory()?;
                    let invoked = std::panic::AssertUnwindSafe(async {
                        let event = common::json_helper::input::web_protocol::<E>(&payload)
                            .map_err(|error| error.to_string())?;
                        handler.handle(event, token).await
                    })
                    .catch_unwind()
                    .await;
                    // 与异步作用域一致：即使解析/执行发生展开型 panic，也先等待释放；
                    // 释放失败优先返回，成功后再把原 panic 交给外层统一报告并重试。
                    let closed = std::panic::AssertUnwindSafe(async { handler.close().await })
                        .catch_unwind()
                        .await;
                    // 在重新展开原 panic 前完成同步析构，避免析构异常形成二次展开。
                    drop(handler);
                    closed.unwrap_or_else(|panic| std::panic::resume_unwind(panic))?;
                    invoked.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                })
            }),
        });
        Ok(())
    }
}

/// 调用方显式提供广播入口。
pub trait EventPublisherProvider {
    fn event_publisher(&self) -> &EventPublisher;
}
