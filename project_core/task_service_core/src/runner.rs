use crate::registry::{QueueTaskContext, ScheduleTaskContext, TaskRegistry};
use crate::sync::{ScheduleConfiguration, TaskRuntimeConfiguration};
use chrono::{DateTime, Utc};
use common::cron_helper::CronExpression;
use distributed_lock::{DistributedLock, LockGuard};
use event_bus::CancellationToken;
use futures_util::FutureExt;
use queue_task_database::QueueTaskStore;
use sea_orm::DbErr;
use std::{
    collections::HashMap,
    future::Future,
    panic::AssertUnwindSafe,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    task::{Id, JoinSet},
    time::{self, Instant},
};

#[derive(Clone)]
pub struct TaskServiceRunnerOptions {
    pub queue_poll_interval_seconds: u64,
    pub schedule_poll_interval_milliseconds: u64,
    pub queue_lock_expiry_seconds: u64,
}
impl Default for TaskServiceRunnerOptions {
    fn default() -> Self {
        Self {
            queue_poll_interval_seconds: 1,
            schedule_poll_interval_milliseconds: 128,
            queue_lock_expiry_seconds: 300,
        }
    }
}

struct RunningTask {
    id: i64,
    name: String,
}
type Executions = JoinSet<()>;
type RunningTasks = HashMap<Id, RunningTask>;

/// 调度只负责执行生命周期；队列状态转换由所属存储实现。
pub struct TaskServiceRunner {
    configuration: TaskRuntimeConfiguration,
    store: Arc<dyn QueueTaskStore>,
    registry: TaskRegistry,
    distributed_lock: Arc<dyn DistributedLock>,
    schedule_trigger_states: Mutex<HashMap<String, DateTime<Utc>>>,
    options: TaskServiceRunnerOptions,
    worker_id: String,
}

impl TaskServiceRunner {
    pub(crate) fn new(
        store: Arc<dyn QueueTaskStore>,
        configuration: TaskRuntimeConfiguration,
        registry: TaskRegistry,
        distributed_lock: Arc<dyn DistributedLock>,
        options: TaskServiceRunnerOptions,
    ) -> Self {
        Self {
            configuration,
            store,
            registry,
            distributed_lock,
            schedule_trigger_states: Mutex::new(HashMap::new()),
            options,
            worker_id: format!("{}-{}", uuid::Uuid::new_v4().simple(), std::process::id()),
        }
    }

    /// 停止信号仅停止新任务领取；已开始的执行和续租保留到宿主剩余期限耗尽。
    pub async fn run(self, stopping: CancellationToken) {
        let runner = Arc::new(self);
        tokio::join!(
            runner.clone().queue_loop(stopping.clone()),
            runner.clone().schedule_loop(stopping.clone()),
            runner
                .configuration
                .synchronize(runner.store.as_ref(), &runner.registry, stopping)
        );
    }

    async fn queue_loop(self: Arc<Self>, stopping: CancellationToken) {
        if self.registry.queue_definitions().is_empty() {
            stopping.cancelled().await;
            return;
        }
        let mut executions = Executions::new();
        let mut running = RunningTasks::new();
        let mut interval = time::interval(Duration::from_secs(
            self.options.queue_poll_interval_seconds.max(1),
        ));
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                _ = stopping.cancelled() => break,
                finished = executions.join_next_with_id(), if !executions.is_empty() => { finish_execution(finished, &mut running); },
                _ = interval.tick() => {
                    if let Err(error) = self.poll_queue_tasks(&stopping, &mut executions, &mut running).await {
                        tracing::error!(target: "task_service.queue.poll", %error, "领取队列任务失败");
                    }
                }
            }
        }
        while let Some(finished) = executions.join_next_with_id().await {
            finish_execution(Some(finished), &mut running);
        }
    }

    async fn poll_queue_tasks(
        self: &Arc<Self>,
        stopping: &CancellationToken,
        executions: &mut Executions,
        running: &mut RunningTasks,
    ) -> Result<(), DbErr> {
        let settings = self.configuration.queue();
        for setting in settings {
            if stopping.is_cancelled() {
                break;
            }
            let semaphore = setting.semaphore;
            if semaphore <= 0 {
                continue;
            }
            let running_ids = running
                .values()
                .filter(|task| task.name == setting.name)
                .map(|task| task.id)
                .collect::<Vec<_>>();
            let capacity = (semaphore as usize).saturating_sub(running_ids.len());
            if capacity == 0 {
                continue;
            }
            let candidates = self
                .store
                .candidates(
                    &setting.name,
                    (capacity as u64).saturating_mul(3),
                    &running_ids,
                )
                .await?;
            let mut started = 0;
            for id in candidates {
                if stopping.is_cancelled() {
                    break;
                }
                if !self
                    .store
                    .try_claim(id, &setting.name, &self.worker_id, self.lease())
                    .await?
                {
                    continue;
                }
                if stopping.is_cancelled() {
                    self.store.release_claim(id, &self.worker_id).await?;
                    break;
                }
                let runner = self.clone();
                let name = setting.name.clone();
                let handle = executions.spawn(async move {
                    if let Err(error) = runner.execute_queue_task(id, &name, semaphore as u32).await {
                        if let Err(save_error) = runner.store.fail(id, &runner.worker_id).await {
                            tracing::error!(target: "task_service.queue.update", task_id = id, %save_error, "失败状态保存失败，等待租约恢复");
                        }
                        tracing::error!(target: "task_service.queue.execution_result", task_id = id, task_name = %name, result = "failed", %error, "队列任务执行失败");
                    }
                });
                running.insert(
                    handle.id(),
                    RunningTask {
                        id,
                        name: setting.name.clone(),
                    },
                );
                started += 1;
                if started >= capacity {
                    break;
                }
            }
        }
        Ok(())
    }

    async fn execute_queue_task(&self, id: i64, name: &str, semaphore: u32) -> Result<(), String> {
        let Some(factory) = self.registry.get_queue_factory(name) else {
            return self
                .store
                .release_claim(id, &self.worker_id)
                .await
                .map_err(|error| error.to_string());
        };
        let Some(lock) = self
            .distributed_lock
            .try_lock(name, self.lease(), semaphore)
            .await
            .map_err(|error| error.to_string())?
        else {
            return self
                .store
                .release_claim(id, &self.worker_id)
                .await
                .map_err(|error| error.to_string());
        };
        let Some(task) = self
            .store
            .start_execution(id, &self.worker_id)
            .await
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        let execution = async {
            let mut handler = std::panic::catch_unwind(AssertUnwindSafe(|| factory()))
                .map_err(|_| "任务处理器工厂发生 panic".to_string())??;
            let result = AssertUnwindSafe(async {
                let returned = handler
                    .execute_with_result(QueueTaskContext {
                        id,
                        name: name.to_owned(),
                        parameter: task.parameter.clone(),
                    })
                    .await?;
                let applied = self
                    .store
                    .complete(
                        id,
                        &self.worker_id,
                        Box::new(move || returned.into_callback_parameter().map_err(DbErr::Custom)),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(applied)
            })
            .catch_unwind()
            .await
            .unwrap_or_else(|_| Err("任务处理器或完成过程发生 panic".into()));
            let closed = AssertUnwindSafe(handler.close())
                .catch_unwind()
                .await
                .unwrap_or_else(|_| Err("任务作用域释放发生 panic".into()));
            if let Err(error) = &closed {
                tracing::error!(target: "task_service.scope.close", task_id = id, task_name = %name, %error, "任务作用域释放失败");
            }
            let applied = result?;
            closed?;
            if applied {
                tracing::info!(target: "task_service.queue.execution_result", task_id = id, task_name = %name, count = task.count, result = "success", "队列任务执行成功");
            }
            Ok(())
        };
        let execution = async {
            let result = AssertUnwindSafe(execution)
                .catch_unwind()
                .await
                .unwrap_or_else(|_| Err("任务执行作用域发生 panic".into()));
            if result.is_err() {
                self.store
                    .fail(id, &self.worker_id)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            result
        };
        self.execute_with_auto_renew(execution, &lock, id, name)
            .await
    }

    fn lease(&self) -> Duration {
        Duration::from_secs(self.options.queue_lock_expiry_seconds.max(1))
    }

    async fn execute_with_auto_renew<F>(
        &self,
        future: F,
        lock: &LockGuard,
        id: i64,
        name: &str,
    ) -> Result<(), String>
    where
        F: Future<Output = Result<(), String>>,
    {
        let lease = self.lease();
        let period = Duration::from_secs((lease.as_secs() / 3).clamp(5, 30));
        let renew = async {
            let mut distributed_lost = false;
            loop {
                time::sleep(period).await;
                if !distributed_lost {
                    match self.distributed_lock.renew(lock, lease).await {
                        Ok(true) => {}
                        other => {
                            distributed_lost = true;
                            tracing::error!(target: "task_service.lock.renew", task_id = id, task_name = %name, result = ?other, "并发锁租约丢失，继续业务执行与数据库续租");
                        }
                    }
                }
                match self.store.renew_lease(id, &self.worker_id, lease).await {
                    Ok(true) => {}
                    Ok(false) => return,
                    Err(error) => {
                        tracing::error!(target: "task_service.lock.renew", task_id = id, %error, "数据库续租失败，将在下轮重试")
                    }
                }
            }
        };
        tokio::pin!(future);
        tokio::select! {
            result = &mut future => result,
            _ = renew => future.await,
        }
    }

    async fn schedule_loop(self: Arc<Self>, stopping: CancellationToken) {
        if self.registry.schedule_definitions().is_empty() {
            stopping.cancelled().await;
            return;
        }
        let mut executions = Executions::new();
        let mut running = RunningTasks::new();
        let mut interval = time::interval_at(
            Instant::now()
                + Duration::from_millis(self.options.schedule_poll_interval_milliseconds.max(1)),
            Duration::from_millis(self.options.schedule_poll_interval_milliseconds.max(1)),
        );
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                _ = stopping.cancelled() => break,
                finished = executions.join_next_with_id(), if !executions.is_empty() => { finish_execution(finished, &mut running); },
                _ = interval.tick() => {
                    if let Err(error) = self.poll_schedule_tasks(&stopping, &mut executions, &mut running).await {
                        tracing::error!(target: "task_service.schedule.poll", %error, "加载定时任务失败");
                    }
                }
            }
        }
        while let Some(finished) = executions.join_next_with_id().await {
            finish_execution(Some(finished), &mut running);
        }
    }

    async fn poll_schedule_tasks(
        &self,
        stopping: &CancellationToken,
        executions: &mut Executions,
        running: &mut RunningTasks,
    ) -> Result<(), DbErr> {
        let settings = self.configuration.schedules();
        for task in settings {
            if stopping.is_cancelled() {
                break;
            }
            let Some(definition) = self.registry.get_schedule_definition(&task.definition_name)
            else {
                continue;
            };
            let Some(factory) = self.registry.get_schedule_factory(&task.definition_name) else {
                continue;
            };
            let is_running = definition.skip_if_running
                && running.values().any(|running| running.name == task.name);
            let Some(trigger) = self.schedule_trigger(&task, is_running) else {
                continue;
            };
            let id = task.id;
            let name = task.name.clone();
            let running_name = name.clone();
            let handle = executions.spawn(async move {
                let result = AssertUnwindSafe(async {
                    let mut handler = factory()?;
                    let result = AssertUnwindSafe(handler.execute(ScheduleTaskContext { name: name.clone(), parameter: task.parameter, cron: task.cron }))
                        .catch_unwind().await.unwrap_or_else(|_| Err("定时任务处理器发生 panic".into()));
                    let closed = AssertUnwindSafe(handler.close()).catch_unwind().await.unwrap_or_else(|_| Err("定时任务作用域释放发生 panic".into()));
                    if let Err(error) = &closed {
                        tracing::error!(target: "task_service.scope.close", task_id = id, task_name = %name, %error, "定时任务作用域释放失败");
                    }
                    result.and(closed)
                }).catch_unwind().await.unwrap_or_else(|_| Err("定时任务处理器工厂发生 panic".into()));
                match result {
                    Ok(()) => tracing::info!(target: "task_service.schedule.execution_result", task_id = id, task_name = %name, result = "success", %trigger, "定时任务执行成功"),
                    Err(error) => tracing::error!(target: "task_service.schedule.execution_result", task_id = id, task_name = %name, result = "failed", %trigger, %error, "定时任务执行失败"),
                }
            });
            running.insert(
                handle.id(),
                RunningTask {
                    id: id.unwrap_or_default(),
                    name: running_name,
                },
            );
        }
        Ok(())
    }

    fn schedule_trigger(
        &self,
        task: &ScheduleConfiguration,
        is_running: bool,
    ) -> Option<DateTime<Utc>> {
        let expression = task.cron.as_deref().unwrap_or_default().trim();
        let schedule = match CronExpression::from_str(expression) {
            Ok(schedule) => schedule,
            Err(error) => {
                tracing::error!(target: "task_service.schedule.trigger", task_id = task.id, %error, "定时任务 Cron 无效");
                self.configuration.disable_schedule(&task.name);
                return None;
            }
        };
        let now = Utc::now();
        let mut states = self
            .schedule_trigger_states
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let last = states
            .entry(task.name.clone())
            .or_insert(now + chrono::Duration::seconds(5));
        let next = match schedule.next_after(*last) {
            Ok(Some(next)) => next,
            result => {
                self.configuration.disable_schedule(&task.name);
                tracing::error!(target: "task_service.schedule.trigger", task_id = task.id, result = ?result, "定时任务无法计算后续触发时间，暂停至下一次配置刷新");
                return None;
            }
        };
        let late = now - next;
        if late > chrono::Duration::milliseconds(256) {
            *last = now;
            return None;
        }
        if late < chrono::Duration::zero() || is_running {
            return None;
        }
        *last = next;
        Some(next)
    }
}

fn finish_execution(
    finished: Option<Result<(Id, ()), tokio::task::JoinError>>,
    running: &mut RunningTasks,
) {
    match finished {
        Some(Ok((id, ()))) => {
            running.remove(&id);
        }
        Some(Err(error)) => {
            running.remove(&error.id());
            tracing::error!(target: "task_service.execution", %error, "任务后台执行异常退出");
        }
        None => {}
    }
}
