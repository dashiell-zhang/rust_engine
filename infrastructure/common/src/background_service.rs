//! 两类宿主共用的常驻服务管理。注册阶段不启动任务，运行失败交回宿主。
use std::{collections::BTreeSet, future::Future, panic::AssertUnwindSafe, pin::Pin};

use cancellation::CancellationToken;
use futures_util::FutureExt;
use tokio::{task::JoinSet, time::Instant};

pub type BackgroundFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

/// 每个注册只运行一次。收到取消后应在 run 返回前完成异步清理；不得脱离宿主管理 spawn。
pub trait BackgroundService: Send + 'static {
    fn run(self: Box<Self>, stopping: CancellationToken) -> BackgroundFuture;
}

impl<F, Fut> BackgroundService for F
where
    F: FnOnce(CancellationToken) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
    fn run(self: Box<Self>, stopping: CancellationToken) -> BackgroundFuture {
        Box::pin(self(stopping))
    }
}

#[derive(Default)]
pub struct BackgroundServiceRegistry {
    names: BTreeSet<&'static str>,
    services: Vec<(&'static str, Box<dyn BackgroundService>)>,
}

impl BackgroundServiceRegistry {
    pub fn register(
        &mut self,
        name: &'static str,
        service: impl BackgroundService,
    ) -> Result<(), String> {
        if name.trim().is_empty() || !self.names.insert(name) {
            return Err(format!("后台服务名称为空或重复: {name}"));
        }
        self.services.push((name, Box::new(service)));
        Ok(())
    }

    /// 只在宿主准备完成后启动；子令牌不允许服务取消宿主或其他服务。
    pub fn start(self, stopping: &CancellationToken) -> BackgroundServices {
        let stopping = stopping.child_token();
        let mut tasks = JoinSet::new();
        for (name, service) in self.services {
            let token = stopping.child_token();
            tasks.spawn(async move {
                let outcome = AssertUnwindSafe(async move { service.run(token).await })
                    .catch_unwind()
                    .await;
                match outcome {
                    Ok(result) => result.map_err(|error| format!("后台服务 {name} 失败: {error}")),
                    Err(_) => Err(format!("后台服务 {name} 发生 panic")),
                }
            });
        }
        BackgroundServices { stopping, tasks }
    }
}

pub struct BackgroundServices {
    stopping: CancellationToken,
    tasks: JoinSet<Result<(), String>>,
}

impl BackgroundServices {
    /// 正常结束不停止宿主。没有活动服务时保持等待，不产生忙循环。
    pub async fn wait_failure(&mut self) -> Result<(), String> {
        while let Some(result) = self.tasks.join_next().await {
            flatten(result)?;
        }
        std::future::pending().await
    }

    pub fn cancel(&self) {
        self.stopping.cancel();
    }

    /// 使用宿主同一绝对期限，不能为每个服务重新分配完整停机时间。
    pub async fn shutdown(&mut self, deadline: Instant) -> Result<(), String> {
        self.cancel();
        let mut failure = Ok(());
        while !self.tasks.is_empty() {
            match tokio::time::timeout_at(deadline, self.tasks.join_next()).await {
                Ok(Some(result)) => failure = failure.and(flatten(result)),
                Ok(None) => break,
                Err(_) => {
                    self.tasks.abort_all();
                    return failure.and(Err("后台服务停机等待超时".into()));
                }
            }
        }
        failure
    }
}

impl Drop for BackgroundServices {
    fn drop(&mut self) {
        self.cancel();
        self.tasks.abort_all();
    }
}

fn flatten(result: Result<Result<(), String>, tokio::task::JoinError>) -> Result<(), String> {
    result.map_err(|_| "后台服务任务异常终止".to_string())?
}
