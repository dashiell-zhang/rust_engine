//! 宿主独占异步清理登记簿。工厂收到同一登记簿，装配中途失败也不会丢失已登记的资源。
use futures_util::FutureExt;
use std::{
    collections::BTreeSet, future::Future, panic::AssertUnwindSafe, pin::Pin, time::Duration,
};
use tokio::time::Instant;

type CleanupFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
type Cleanup = Box<dyn FnOnce(Instant) -> CleanupFuture + Send>;

#[derive(Default)]
pub struct ResourceCleanup {
    names: BTreeSet<String>,
    entries: Vec<(String, Cleanup)>,
    deadline: Option<Instant>,
    closed: bool,
}

impl ResourceCleanup {
    /// 在打开连接之前拒绝重复名称；创建成功到登记之间没有可取消的 await 点。
    pub async fn acquire<T, Create, F, Fut>(
        &mut self,
        name: impl Into<String>,
        create: Create,
        cleanup: F,
    ) -> Result<T, String>
    where
        T: Clone + Send + 'static,
        Create: Future<Output = Result<T, String>>,
        F: FnOnce(T, Instant) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let name = name.into();
        if self.closed
            || self.deadline.is_some()
            || name.trim().is_empty()
            || self.names.contains(&name)
        {
            return Err(format!("不能创建已登记或停机中的资源: {name}"));
        }
        let value = create.await?;
        self.manage(name, value, cleanup)
    }
    /// 按依赖先、消费者后的顺序登记，退出逆序清理。名称必须唯一且不含连接凭据。
    pub fn register<F, Fut>(&mut self, name: impl Into<String>, cleanup: F) -> Result<(), String>
    where
        F: FnOnce(Instant) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let name = name.into();
        if self.closed || self.deadline.is_some() {
            return Err("宿主已开始退出，不能再登记资源".into());
        }
        if name.trim().is_empty() || !self.names.insert(name.clone()) {
            return Err(format!("资源清理名称为空或重复: {name}"));
        }
        self.entries
            .push((name, Box::new(move |deadline| Box::pin(cleanup(deadline)))));
        Ok(())
    }

    /// 登记共享句柄的清理所有权并返回原值；被拒绝的值未被登记，仍按普通 Drop 处理。
    pub fn manage<T, F, Fut>(
        &mut self,
        name: impl Into<String>,
        value: T,
        cleanup: F,
    ) -> Result<T, String>
    where
        T: Clone + Send + 'static,
        F: FnOnce(T, Instant) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let owned = value.clone();
        self.register(name, move |deadline| cleanup(owned, deadline))?;
        Ok(value)
    }

    /// 整个宿主只创建一次停机期限，包括启动取消后的清理。
    pub fn begin_shutdown(&mut self) -> Instant {
        *self
            .deadline
            .get_or_insert_with(|| Instant::now() + Duration::from_secs(30))
    }

    /// 必须由宿主显式 await；不在 Drop 中启动脱离宿主管理的异步任务。
    /// 回调只调用一次；普通失败、panic、超时均保留诊断，并继续尝试剩余回调。
    pub async fn shutdown(&mut self, deadline: Instant) -> Result<(), String> {
        self.closed = true;
        let deadline = self
            .deadline
            .map_or(deadline, |existing| existing.min(deadline));
        self.deadline = Some(deadline);
        let mut errors = Vec::new();
        while let Some((name, cleanup)) = self.entries.pop() {
            let future = match std::panic::catch_unwind(AssertUnwindSafe(|| cleanup(deadline))) {
                Ok(future) => AssertUnwindSafe(future).catch_unwind(),
                Err(_) => {
                    errors.push(format!("资源 {name} 清理 panic"));
                    continue;
                }
            };
            let failure = match tokio::time::timeout_at(deadline, future).await {
                Ok(Ok(Ok(()))) => None,
                Ok(Ok(Err(_))) => Some("失败"),
                Ok(Err(_)) => Some("panic"),
                Err(_) => Some("超时"),
            };
            if let Some(failure) = failure {
                // 不输出回调携带的任意错误正文，避免资源连接信息进入停机诊断。
                errors.push(format!("资源 {name} 清理{failure}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("；"))
        }
    }
}

/// 保留原始失败，也保留清理失败；不能用 Result::and 丢掉后者。
pub fn combine<T>(result: Result<T, String>, cleanup: Result<(), String>) -> Result<T, String> {
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(format!("{error}；{cleanup}")),
    }
}
