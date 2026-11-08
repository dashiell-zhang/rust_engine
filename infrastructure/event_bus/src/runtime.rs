use crate::{
    CancellationToken, EventBusOptions, EventListenerRegistry, EventRecord, EventResult,
    EventTransport,
};
use chrono::{DateTime, Utc};
use futures_util::FutureExt;
use std::{sync::Arc, time::Duration};
use tokio::{task::JoinHandle, time::sleep};

/// 先固定启动位置，再由宿主完成业务初始化，最后启动监听循环。
pub struct PreparedEventBus {
    transport: Arc<dyn EventTransport>,
    options: EventBusOptions,
    position: String,
}
impl PreparedEventBus {
    pub async fn prepare(
        transport: Arc<dyn EventTransport>,
        options: EventBusOptions,
        stopping: &CancellationToken,
    ) -> EventResult<Self> {
        options.validate()?;
        loop {
            let result = transport.position_with_cancellation(stopping).await;
            if stopping.is_cancelled() {
                return Err("事件总线启动已取消".into());
            }
            match result {
                Ok(position) => {
                    return Ok(Self {
                        transport,
                        options,
                        position,
                    });
                }
                Err(error) => {
                    tracing::error!(target: "event_bus.startup", %error, scope = %options.scope, "等待事件后端恢复")
                }
            }
            tokio::select! { _ = stopping.cancelled() => return Err("事件总线启动已取消".into()),
            _ = sleep(Duration::from_millis(options.poll_interval_milliseconds)) => {} }
        }
    }
    pub fn start(self, registry: EventListenerRegistry) -> EventRuntime {
        let stopping = CancellationToken::new();
        let transport = self.transport.clone();
        let options = self.options.clone();
        let read_token = stopping.clone();
        let clean_token = stopping.clone();
        let read = tokio::spawn(async move { self.run(registry, read_token).await });
        let clean = tokio::spawn(async move { cleanup(transport, options, clean_token).await });
        EventRuntime {
            stopping,
            read: Some(read),
            clean: Some(clean),
        }
    }
    async fn run(mut self, registry: EventListenerRegistry, stopping: CancellationToken) {
        let mut last_read = Utc::now();
        loop {
            let result = self
                .transport
                .read_with_cancellation(&self.position, self.options.batch_size, &stopping)
                .await;
            if stopping.is_cancelled() {
                return;
            }
            match result {
                Ok(records) => {
                    let now = Utc::now();
                    if (now - last_read).num_milliseconds()
                        > self.options.retention_milliseconds as i64
                    {
                        tracing::warn!(target: "event_bus.read", scope = %self.options.scope, "断线时间超过事件保留时间，可能有事件已清理");
                    }
                    last_read = now;
                    let full = records.len() == self.options.batch_size;
                    for record in records {
                        dispatch(&record, &registry, &self.options, &stopping).await;
                        if stopping.is_cancelled() {
                            return;
                        }
                        self.position = record.position;
                    }
                    if full {
                        continue;
                    }
                }
                Err(error) => {
                    tracing::error!(target: "event_bus.read", %error, position = %self.position, "保留事件游标并重试")
                }
            }
            tokio::select! { _ = stopping.cancelled() => return,
            _ = sleep(Duration::from_millis(self.options.poll_interval_milliseconds)) => {} }
        }
    }
}

/// 保存已成功监听的进度，只重试失败监听；同一进程按记录顺序派发。
async fn dispatch(
    record: &EventRecord,
    registry: &EventListenerRegistry,
    options: &EventBusOptions,
    stopping: &CancellationToken,
) {
    let Some(message) = &record.message else {
        return;
    };
    let Some(listeners) = registry.listeners.get(&message.event_name) else {
        return;
    };
    let event_token = stopping.child_token();
    let mut pending = (0..listeners.len()).collect::<Vec<_>>();
    let dispatch = async {
        while !pending.is_empty() {
            let mut failed = Vec::new();
            for index in pending.drain(..) {
                if event_token.is_cancelled() {
                    return;
                }
                let listener = &listeners[index];
                let invocation = std::panic::AssertUnwindSafe(async {
                    (listener.invoke)(message.payload.clone(), event_token.clone()).await
                })
                .catch_unwind()
                .await;
                let result = invocation.unwrap_or_else(|panic| {
                    let detail = panic
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| panic.downcast_ref::<&str>().copied())
                        .unwrap_or("未知 panic");
                    Err(format!("事件监听 panic: {detail}"))
                });
                if let Err(error) = result {
                    tracing::error!(target: "event_bus.dispatch", %error, event_id = %message.event_id, handler = %listener.identity, "事件监听失败，将重试");
                    failed.push(index);
                }
            }
            pending = failed;
            if !pending.is_empty() {
                tokio::select! { _ = event_token.cancelled() => return,
                _ = sleep(Duration::from_millis(options.retry_interval_milliseconds)) => {} }
            }
        }
    };
    tokio::pin!(dispatch);
    tokio::select! {
        biased;
        _ = stopping.cancelled() => { event_token.cancel(); dispatch.await; }
        _ = expires(record.published_at, options.retention_milliseconds) => {
            event_token.cancel();
            tracing::warn!(target: "event_bus.dispatch", event_id = %message.event_id, "事件到期，取消剩余监听");
            // 监听通过令牌协作退出；不得在调用结束前释放它拥有的资源。
            dispatch.await;
        }
        _ = &mut dispatch => {}
    }
}
async fn expires(published: DateTime<Utc>, retention: u64) {
    loop {
        let age = (Utc::now() - published).num_milliseconds().max(0) as u64;
        let remaining = retention.saturating_sub(age);
        if remaining == 0 {
            return;
        }
        sleep(Duration::from_millis(remaining.min(4_294_967_294))).await;
    }
}
async fn cleanup(
    transport: Arc<dyn EventTransport>,
    options: EventBusOptions,
    stopping: CancellationToken,
) {
    // DateTimeOffset 的最小 UTC 日期；Chrono 的负年份不能作为源协议的清理边界。
    const MINIMUM: DateTime<Utc> =
        DateTime::from_timestamp(-62_135_596_800, 0).expect("0001-01-01 UTC");
    loop {
        let before = Utc::now()
            .checked_sub_signed(chrono::Duration::milliseconds(
                options.retention_milliseconds as i64,
            ))
            .filter(|cutoff| *cutoff >= MINIMUM)
            .unwrap_or(MINIMUM);
        let result = transport.trim_with_cancellation(before, &stopping).await;
        if stopping.is_cancelled() {
            return;
        }
        if let Err(error) = result {
            tracing::error!(target: "event_bus.cleanup", %error, "事件清理失败，将重试");
        }
        tokio::select! { _ = stopping.cancelled() => return,
        _ = sleep(Duration::from_millis(options.cleanup_interval_milliseconds)) => {} }
    }
}

pub struct EventRuntime {
    stopping: CancellationToken,
    read: Option<JoinHandle<()>>,
    clean: Option<JoinHandle<()>>,
}
impl EventRuntime {
    pub fn cancel(&self) {
        self.stopping.cancel();
    }

    /// 后台循环的意外退出报告给宿主，由宿主结束服务。
    pub async fn wait_failure(&mut self) -> EventResult<()> {
        let result = tokio::select! { result = self.read.as_mut().expect("事件读取任务存在") => result,
        result = self.clean.as_mut().expect("事件清理任务存在") => result };
        self.stopping.cancel();
        Err(match result {
            Ok(()) => "事件后台任务意外停止".into(),
            Err(error) => format!("事件后台任务退出: {error}"),
        })
    }
    pub async fn shutdown(&mut self) {
        self.stopping.cancel();
        for handle in [&mut self.read, &mut self.clean] {
            if let Some(task) = handle.as_mut()
                && !task.is_finished()
            {
                let _ = task.await;
            }
            handle.take();
        }
    }
}
impl Drop for EventRuntime {
    fn drop(&mut self) {
        self.stopping.cancel();
        // 正常宿主退出使用 shutdown 等待；异常丢弃时取消，避免脱离宿主继续运行。
        if let Some(handle) = &self.read {
            handle.abort();
        }
        if let Some(handle) = &self.clean {
            handle.abort();
        }
    }
}
