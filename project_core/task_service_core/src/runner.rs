use application_model::task_center::ARGS_DEFAULT_PARAMETER;
use chrono::{DateTime, Utc};
use common::identifier_generator::IdentifierGenerator;
use cron::Schedule;
use distributed_lock::{DistributedLock, LockGuard};
use repository::{
    extensions::SoftDeleteQueryExt, log, queue_task, queue_task::QueueTaskStatus, task_setting,
};
use sea_orm::{
    ActiveEnum, ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    ExprTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
    sea_query::Expr,
};
use serde_json::json;
use std::{
    collections::HashMap,
    future::Future,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration as StdDuration,
};
use tokio::{
    signal,
    time::{self, Duration, Instant},
};

use crate::registry::{QueueTaskContext, ScheduleTaskContext, TaskRegistry};

/// 任务轮询配置
#[derive(Clone)]
pub struct TaskServiceRunnerOptions {
    /// 队列任务轮询间隔秒数
    pub queue_poll_interval_seconds: u64,

    /// 定时任务轮询间隔毫秒数
    pub schedule_poll_interval_milliseconds: u64,

    /// 队列任务执行锁过期秒数
    pub queue_lock_expiry_seconds: u64,

    /// 定时任务执行锁过期秒数
    pub schedule_lock_expiry_seconds: u64,

    /// 日志项目标识
    pub log_project: String,
}

impl Default for TaskServiceRunnerOptions {
    /// 创建默认轮询配置
    fn default() -> Self {
        Self {
            queue_poll_interval_seconds: 5,
            schedule_poll_interval_milliseconds: 256,
            queue_lock_expiry_seconds: 60,
            schedule_lock_expiry_seconds: 60,
            log_project: "task_service".to_string(),
        }
    }
}

/// 任务宿主运行器
pub struct TaskServiceRunner {
    /// 数据库连接
    db: Arc<DatabaseConnection>,

    /// 任务注册表
    registry: TaskRegistry,

    /// 分布式锁
    distributed_lock: Arc<dyn DistributedLock>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,

    /// 定时任务最近触发标记
    ///
    /// 用于避免同一进程在同一个触发时间片内重复执行同一个定时任务
    schedule_trigger_states: Mutex<HashMap<i64, String>>,

    /// 轮询配置
    options: TaskServiceRunnerOptions,

    /// 当前 TaskService 实例标识
    worker_id: String,
}

impl TaskServiceRunner {
    /// 创建任务宿主运行器
    pub fn new(
        db: Arc<DatabaseConnection>,
        registry: TaskRegistry,
        distributed_lock: Arc<dyn DistributedLock>,
        identifier_generator: IdentifierGenerator,
        options: TaskServiceRunnerOptions,
    ) -> Self {
        Self {
            db,
            registry,
            distributed_lock,
            identifier_generator,
            schedule_trigger_states: Mutex::new(HashMap::new()),
            options,
            worker_id: create_worker_id(),
        }
    }

    /// 启动任务轮询循环
    pub async fn run(self) {
        tracing::info!(
            target: "task_service.registry",
            queue_names = ?self.registry.queue_names(),
            schedule_names = ?self.registry.schedule_names(),
            "任务注册表加载完成"
        );

        tracing::info!(target: "task_service.startup", "TaskService 轮询器启动完成");

        let mut queue_interval = time::interval(Duration::from_secs(
            self.options.queue_poll_interval_seconds,
        ));
        let mut schedule_interval = time::interval(Duration::from_millis(
            self.options.schedule_poll_interval_milliseconds,
        ));

        queue_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
        schedule_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = queue_interval.tick() => {
                    self.poll_queue_tasks().await;
                }
                _ = schedule_interval.tick() => {
                    self.poll_schedule_tasks().await;
                }
                result = signal::ctrl_c() => {
                    match result {
                        Ok(_) => tracing::info!(target: "task_service.shutdown", "收到退出信号，TaskService 即将停止"),
                        Err(error) => tracing::error!(target: "task_service.shutdown", error = %error, "监听退出信号失败，TaskService 即将停止"),
                    }
                    break;
                }
            }
        }
    }

    /// 轮询待执行的队列任务
    async fn poll_queue_tasks(&self) {
        let now = Utc::now();

        let count_result = queue_task::Entity::find()
            .not_deleted(queue_task::Column::DeleteTime)
            .filter(
                queue_task::Column::Status.eq(QueueTaskStatus::Pending).or(
                    queue_task::Column::Status.eq(QueueTaskStatus::Running).and(
                        queue_task::Column::LeaseExpireTime
                            .is_not_null()
                            .and(queue_task::Column::LeaseExpireTime.lte(now)),
                    ),
                ),
            )
            .filter(queue_task::Column::Count.lt(3))
            .filter(queue_task::Column::CreateTime.lt(now))
            .filter(
                queue_task::Column::PlanTime
                    .is_null()
                    .or(queue_task::Column::PlanTime.lte(now)),
            )
            .count(self.db.as_ref())
            .await;

        match count_result {
            Ok(count) => {
                tracing::info!(
                    target: "task_service.queue.poll",
                    pending_count = count,
                    "队列任务轮询完成"
                );
            }
            Err(error) => {
                tracing::error!(
                    target: "task_service.queue.poll",
                    error = %error,
                    "队列任务轮询失败"
                );
            }
        }

        let task_result = queue_task::Entity::find()
            .not_deleted(queue_task::Column::DeleteTime)
            .filter(
                queue_task::Column::Status.eq(QueueTaskStatus::Pending).or(
                    queue_task::Column::Status.eq(QueueTaskStatus::Running).and(
                        queue_task::Column::LeaseExpireTime
                            .is_not_null()
                            .and(queue_task::Column::LeaseExpireTime.lte(now)),
                    ),
                ),
            )
            .filter(queue_task::Column::Count.lt(3))
            .filter(queue_task::Column::CreateTime.lt(now))
            .filter(
                queue_task::Column::PlanTime
                    .is_null()
                    .or(queue_task::Column::PlanTime.lte(now)),
            )
            .order_by_asc(queue_task::Column::PlanTime)
            .order_by_asc(queue_task::Column::Count)
            .order_by_asc(queue_task::Column::Id)
            .limit(20)
            .all(self.db.as_ref())
            .await;

        match task_result {
            Ok(tasks) => {
                let mut matched_names = Vec::new();
                let mut missing_names = Vec::new();

                for task in tasks {
                    if self.registry.get_queue_handler(&task.name).is_some() {
                        matched_names.push(task.name.clone());
                        self.execute_queue_task(task).await;
                    } else {
                        missing_names.push(task.name);
                    }
                }

                if !matched_names.is_empty() {
                    tracing::info!(
                        target: "task_service.queue.registry",
                        matched_names = ?matched_names,
                        "队列任务已匹配到处理器"
                    );
                }

                if !missing_names.is_empty() {
                    tracing::warn!(
                        target: "task_service.queue.registry",
                        missing_names = ?missing_names,
                        "存在未注册的队列任务处理器"
                    );
                }
            }
            Err(error) => {
                tracing::error!(
                    target: "task_service.queue.registry",
                    error = %error,
                    "加载待执行队列任务失败"
                );
            }
        }
    }

    /// 执行单个队列任务
    async fn execute_queue_task(&self, task: queue_task::Model) {
        let Some(handler) = self.registry.get_queue_handler(&task.name) else {
            return;
        };

        let queue_definition = self.registry.get_queue_definition(&task.name);
        let semaphore = queue_definition
            .as_ref()
            .map(|item| Ord::max(item.semaphore, 1) as u32)
            .unwrap_or(1);
        let lock_key = format!("queue_task_worker:{}", task.name);
        let lock_guard = match self
            .distributed_lock
            .try_lock(
                &lock_key,
                StdDuration::from_secs(self.options.queue_lock_expiry_seconds),
                semaphore,
            )
            .await
        {
            Ok(Some(guard)) => guard,
            Ok(None) => {
                tracing::info!(
                    target: "task_service.queue.lock",
                    task_id = task.id,
                    task_name = %task.name,
                    lock_key = %lock_key,
                    "队列任务已被其他执行器占用，当前轮询跳过"
                );
                return;
            }
            Err(error) => {
                tracing::error!(
                    target: "task_service.queue.lock",
                    task_id = task.id,
                    task_name = %task.name,
                    lock_key = %lock_key,
                    error = %error,
                    "获取队列任务执行锁失败"
                );
                return;
            }
        };

        let Some(claimed_task) = self.try_claim_queue_task(&task).await else {
            drop(lock_guard);
            return;
        };

        tracing::info!(
            target: "task_service.queue.execute",
            task_id = claimed_task.id,
            task_name = %claimed_task.name,
            parameter = ?claimed_task.parameter,
            "开始执行队列任务"
        );

        let execute_result = handler.execute(QueueTaskContext {
            id: claimed_task.id,
            name: claimed_task.name.clone(),
            parameter: claimed_task.parameter.clone(),
        });

        let execute_result = self
            .execute_with_auto_renew(
                execute_result,
                &lock_guard,
                self.options.queue_lock_expiry_seconds,
                claimed_task.id,
                &claimed_task.name,
                true,
            )
            .await;

        match self
            .update_queue_task_execution_result(&claimed_task, execute_result)
            .await
        {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(
                    target: "task_service.queue.update",
                    task_id = claimed_task.id,
                    task_name = %claimed_task.name,
                    error = %error,
                    "队列任务状态回写失败"
                );
            }
        }

        drop(lock_guard);
    }

    /// 尝试领取队列任务
    /// 通过单条条件更新保证同一时刻只有一个实例可以成功领取
    async fn try_claim_queue_task(&self, task: &queue_task::Model) -> Option<queue_task::Model> {
        let now = Utc::now();
        let lease_expire_time =
            now + chrono::Duration::seconds(self.options.queue_lock_expiry_seconds as i64);

        let update_result = queue_task::Entity::update_many()
            .col_expr(
                queue_task::Column::Status,
                Expr::value(QueueTaskStatus::Running.to_value()),
            )
            .col_expr(
                queue_task::Column::WorkerId,
                Expr::value(Some(self.worker_id.clone())),
            )
            .col_expr(
                queue_task::Column::LeaseExpireTime,
                Expr::value(Some(lease_expire_time)),
            )
            .filter(queue_task::Column::Id.eq(task.id))
            .filter(queue_task::Column::Name.eq(task.name.as_str()))
            .not_deleted(queue_task::Column::DeleteTime)
            .filter(queue_task::Column::Count.lt(3))
            .filter(queue_task::Column::SuccessTime.is_null())
            .filter(queue_task::Column::CreateTime.lt(now))
            .filter(
                queue_task::Column::PlanTime
                    .is_null()
                    .or(queue_task::Column::PlanTime.lte(now)),
            )
            .filter(
                queue_task::Column::Status.eq(QueueTaskStatus::Pending).or(
                    queue_task::Column::Status.eq(QueueTaskStatus::Running).and(
                        queue_task::Column::LeaseExpireTime
                            .is_not_null()
                            .and(queue_task::Column::LeaseExpireTime.lte(now)),
                    ),
                ),
            )
            .exec(self.db.as_ref())
            .await;

        let update_result = match update_result {
            Ok(result) => result,
            Err(error) => {
                tracing::error!(
                    target: "task_service.queue.claim",
                    task_id = task.id,
                    task_name = %task.name,
                    error = %error,
                    "领取队列任务失败"
                );
                return None;
            }
        };

        if update_result.rows_affected != 1 {
            return None;
        }

        queue_task::Entity::find_by_id(task.id)
            .filter(queue_task::Column::WorkerId.eq(self.worker_id.as_str()))
            .filter(queue_task::Column::Status.eq(QueueTaskStatus::Running))
            .not_deleted(queue_task::Column::DeleteTime)
            .one(self.db.as_ref())
            .await
            .ok()
            .flatten()
    }

    /// 回写队列任务执行结果
    async fn update_queue_task_execution_result(
        &self,
        task: &queue_task::Model,
        execute_result: Result<(), String>,
    ) -> Result<(), sea_orm::DbErr> {
        let now = Utc::now();
        let mut active = task.clone().into_active_model();

        if task.first_time.is_none() {
            active.first_time = Set(Some(now.into()));
        }

        active.last_time = Set(Some(now.into()));
        active.lease_expire_time = Set(None);

        match execute_result {
            Ok(()) => {
                let has_child = queue_task::Entity::find()
                    .not_deleted(queue_task::Column::DeleteTime)
                    .filter(queue_task::Column::ParentTaskId.eq(task.id))
                    .count(self.db.as_ref())
                    .await?
                    > 0;

                active.status = Set(QueueTaskStatus::Succeeded);
                active.success_time = Set(Some(now.into()));
                active.child_success_time = Set(if has_child {
                    task.child_success_time
                } else {
                    Some(now.into())
                });
                active.worker_id = Set(Some(self.worker_id.clone()));

                active.update(self.db.as_ref()).await?;

                let callback_parameter = task.callback_parameter.clone();
                let child_success_time = if has_child {
                    task.child_success_time
                } else {
                    Some(now.into())
                };

                if let Some(callback_name) = task.callback_name.as_deref()
                    && child_success_time.is_some()
                {
                    self.create_callback_queue_task(callback_name, callback_parameter)
                        .await?;
                }

                if let Some(parent_task_id) = task.parent_task_id
                    && child_success_time.is_some()
                {
                    self.mark_parent_child_success(parent_task_id).await?;
                }

                tracing::info!(
                    target: "task_service.queue.execute",
                    task_id = task.id,
                    task_name = %task.name,
                    "队列任务执行成功"
                );

                self.write_execution_log(
                    "INFO",
                    "task_service.queue.execution_result",
                    json!({
                        "task_id": task.id,
                        "task_name": task.name,
                        "result": "success",
                        "count": task.count,
                        "success_time": now.to_rfc3339(),
                        "callback_name": task.callback_name,
                        "parent_task_id": task.parent_task_id,
                    })
                    .to_string(),
                )
                .await?;
            }
            Err(error_message) => {
                // 按重试次数线性递增等待时长，第 n 次失败将在 n*5 分钟后重新入队
                let new_count = task.count.saturating_add(1);
                let retry_delay_minutes = i64::from(new_count) * 5;
                let next_plan_time = now + chrono::Duration::minutes(retry_delay_minutes);

                active.status = Set(if new_count >= 3 {
                    QueueTaskStatus::Failed
                } else {
                    QueueTaskStatus::Pending
                });
                active.count = Set(new_count);
                active.plan_time = Set(if new_count >= 3 {
                    task.plan_time
                } else {
                    Some(next_plan_time.into())
                });
                active.worker_id = Set(Some(self.worker_id.clone()));

                active.update(self.db.as_ref()).await?;

                tracing::error!(
                    target: "task_service.queue.execute",
                    task_id = task.id,
                    task_name = %task.name,
                    error = %error_message,
                    retry_count = new_count,
                    "队列任务执行失败"
                );

                self.write_execution_log(
                    "ERROR",
                    "task_service.queue.execution_result",
                    json!({
                        "task_id": task.id,
                        "task_name": task.name,
                        "result": "failed",
                        "status": if new_count >= 3 { "Failed" } else { "Pending" },
                        "retry_count": new_count,
                        "retry_plan_time": if new_count >= 3 { None::<String> } else { Some(next_plan_time.to_rfc3339()) },
                        "error": error_message,
                        "parameter": task.parameter,
                        "callback_name": task.callback_name,
                        "parent_task_id": task.parent_task_id,
                    })
                    .to_string(),
                )
                .await?;
            }
        }

        Ok(())
    }

    /// 创建回调队列任务
    async fn create_callback_queue_task(
        &self,
        callback_name: &str,
        callback_parameter: Option<String>,
    ) -> Result<(), sea_orm::DbErr> {
        let callback_task_id =
            self.identifier_generator.get_id().await.map_err(|error| {
                sea_orm::DbErr::Custom(format!("生成回调任务 ID 失败: {error}"))
            })?;

        queue_task::ActiveModel {
            id: Set(callback_task_id),
            name: Set(callback_name.to_string()),
            parameter: Set(callback_parameter),
            status: Set(QueueTaskStatus::Pending),
            count: Set(0),
            first_time: Set(None),
            last_time: Set(None),
            success_time: Set(None),
            create_time: Set(Utc::now().into()),
            delete_time: Set(None),
            plan_time: Set(None),
            worker_id: Set(None),
            lease_expire_time: Set(None),
            callback_name: Set(None),
            callback_parameter: Set(None),
            child_success_time: Set(None),
            parent_task_id: Set(None),
        }
        .insert(self.db.as_ref())
        .await
        .map(|_| ())
    }

    /// 递归回写父任务子链完成状态
    async fn mark_parent_child_success(&self, parent_task_id: i64) -> Result<(), sea_orm::DbErr> {
        let mut current_parent_task_id = Some(parent_task_id);

        while let Some(task_id) = current_parent_task_id {
            let lock_key = format!("queue_task_parent:{task_id}");
            let _lock_guard = self
                .distributed_lock
                .lock(
                    &lock_key,
                    StdDuration::from_secs(self.options.queue_lock_expiry_seconds),
                    1,
                )
                .await
                .map_err(|error| {
                    sea_orm::DbErr::Custom(format!("获取父任务状态锁失败: {error}"))
                })?;

            let has_pending_sibling = queue_task::Entity::find()
                .not_deleted(queue_task::Column::DeleteTime)
                .filter(queue_task::Column::ParentTaskId.eq(task_id))
                .filter(queue_task::Column::ChildSuccessTime.is_null())
                .count(self.db.as_ref())
                .await?
                > 0;

            if has_pending_sibling {
                break;
            }

            let parent_task = queue_task::Entity::find_by_id(task_id)
                .not_deleted(queue_task::Column::DeleteTime)
                .one(self.db.as_ref())
                .await?;

            let Some(parent_task) = parent_task else {
                break;
            };

            if parent_task.child_success_time.is_some() {
                break;
            }

            let now = Utc::now();
            let mut active = parent_task.clone().into_active_model();
            active.child_success_time = Set(Some(now.into()));
            active.update(self.db.as_ref()).await?;

            if let Some(callback_name) = parent_task.callback_name.as_deref() {
                self.create_callback_queue_task(
                    callback_name,
                    parent_task.callback_parameter.clone(),
                )
                .await?;
            }

            current_parent_task_id = parent_task.parent_task_id;
        }

        Ok(())
    }

    /// 轮询已启用的定时任务
    async fn poll_schedule_tasks(&self) {
        let count_result = task_setting::Entity::find()
            .not_deleted(task_setting::Column::DeleteTime)
            .filter(task_setting::Column::Category.eq("ScheduleTask"))
            .filter(task_setting::Column::IsEnable.eq(true))
            .filter(
                task_setting::Column::Parameter
                    .is_null()
                    .or(task_setting::Column::Parameter.ne(ARGS_DEFAULT_PARAMETER)),
            )
            .count(self.db.as_ref())
            .await;

        match count_result {
            Ok(count) => {
                tracing::info!(
                    target: "task_service.schedule.poll",
                    enabled_count = count,
                    "定时任务轮询完成"
                );
            }
            Err(error) => {
                tracing::error!(
                    target: "task_service.schedule.poll",
                    error = %error,
                    "定时任务轮询失败"
                );
            }
        }

        let task_result = task_setting::Entity::find()
            .not_deleted(task_setting::Column::DeleteTime)
            .filter(task_setting::Column::Category.eq("ScheduleTask"))
            .filter(task_setting::Column::IsEnable.eq(true))
            .filter(
                task_setting::Column::Parameter
                    .is_null()
                    .or(task_setting::Column::Parameter.ne(ARGS_DEFAULT_PARAMETER)),
            )
            .order_by_asc(task_setting::Column::Id)
            .limit(20)
            .all(self.db.as_ref())
            .await;

        match task_result {
            Ok(tasks) => {
                let mut matched_names = Vec::new();
                let mut missing_names = Vec::new();

                for task in tasks {
                    if self.registry.get_schedule_handler(&task.name).is_some() {
                        matched_names.push(task.name.clone());
                        self.try_execute_schedule_task(task).await;
                    } else {
                        missing_names.push(task.name);
                    }
                }

                if !matched_names.is_empty() {
                    tracing::info!(
                        target: "task_service.schedule.registry",
                        matched_names = ?matched_names,
                        "定时任务已匹配到处理器"
                    );
                }

                if !missing_names.is_empty() {
                    tracing::warn!(
                        target: "task_service.schedule.registry",
                        missing_names = ?missing_names,
                        "存在未注册的定时任务处理器"
                    );
                }
            }
            Err(error) => {
                tracing::error!(
                    target: "task_service.schedule.registry",
                    error = %error,
                    "加载已启用定时任务失败"
                );
            }
        }
    }

    /// 尝试执行单个定时任务
    async fn try_execute_schedule_task(&self, task: task_setting::Model) {
        let Some(trigger_key) = self.try_get_schedule_trigger_key(&task) else {
            return;
        };

        let Some(handler) = self.registry.get_schedule_handler(&task.name) else {
            return;
        };

        let schedule_definition = self.registry.get_schedule_definition(&task.name);

        let lock_key = format!("schedule_task:{}:{}", task.id, trigger_key);
        let lock_guard = match self
            .distributed_lock
            .try_lock(
                &lock_key,
                StdDuration::from_secs(self.options.schedule_lock_expiry_seconds),
                1,
            )
            .await
        {
            Ok(Some(guard)) => guard,
            Ok(None) => {
                tracing::info!(
                    target: "task_service.schedule.lock",
                    task_id = task.id,
                    task_name = %task.name,
                    lock_key = %lock_key,
                    "定时任务已被其他执行器占用，当前轮询跳过"
                );
                return;
            }
            Err(error) => {
                tracing::error!(
                    target: "task_service.schedule.lock",
                    task_id = task.id,
                    task_name = %task.name,
                    lock_key = %lock_key,
                    error = %error,
                    "获取定时任务执行锁失败"
                );
                return;
            }
        };

        if !self.try_mark_schedule_trigger(task.id, &trigger_key) {
            return;
        }

        tracing::info!(
            target: "task_service.schedule.execute",
            task_id = task.id,
            task_name = %task.name,
            trigger_key = %trigger_key,
            parameter = ?task.parameter,
            cron = ?task.cron,
            "开始执行定时任务"
        );

        let execute_result = handler.execute(ScheduleTaskContext {
            name: task.name.clone(),
            parameter: task.parameter.clone(),
            cron: task.cron.clone(),
        });

        let execute_result = self
            .execute_with_auto_renew(
                execute_result,
                &lock_guard,
                self.options.schedule_lock_expiry_seconds,
                task.id,
                &task.name,
                false,
            )
            .await;

        match execute_result {
            Ok(()) => {
                let finished_at = Utc::now();

                tracing::info!(
                    target: "task_service.schedule.execute",
                    task_id = task.id,
                    task_name = %task.name,
                    trigger_key = %trigger_key,
                    "定时任务执行成功"
                );

                if let Err(error) = self
                    .write_execution_log(
                        "INFO",
                        "task_service.schedule.execution_result",
                        json!({
                            "task_id": task.id,
                            "task_name": task.name.clone(),
                            "result": "success",
                            "trigger_key": trigger_key,
                            "parameter": task.parameter.clone(),
                            "cron": task.cron.clone(),
                            "finished_at": finished_at,
                        })
                        .to_string(),
                    )
                    .await
                {
                    tracing::error!(
                        target: "task_service.schedule.log",
                        task_id = task.id,
                        task_name = %task.name,
                        error = %error,
                        "写入定时任务执行日志失败"
                    );
                }
            }
            Err(error_message) => {
                let finished_at = Utc::now();

                tracing::error!(
                    target: "task_service.schedule.execute",
                    task_id = task.id,
                    task_name = %task.name,
                    trigger_key = %trigger_key,
                    error = %error_message,
                    "定时任务执行失败"
                );

                if let Err(error) = self
                    .write_execution_log(
                        "ERROR",
                        "task_service.schedule.execution_result",
                        json!({
                            "task_id": task.id,
                            "task_name": task.name.clone(),
                            "result": "failed",
                            "trigger_key": trigger_key,
                            "error": error_message.clone(),
                            "parameter": task.parameter.clone(),
                            "cron": task.cron.clone(),
                            "finished_at": finished_at,
                        })
                        .to_string(),
                    )
                    .await
                {
                    tracing::error!(
                        target: "task_service.schedule.log",
                        task_id = task.id,
                        task_name = %task.name,
                        error = %error,
                        "写入定时任务执行日志失败"
                    );
                }
            }
        }

        if schedule_definition.is_some_and(|definition| definition.skip_if_running) {
            tracing::debug!(
                target: "task_service.schedule.execute",
                task_id = task.id,
                task_name = %task.name,
                "旧表结构下不持久化 skip_if_running，仅保留注册表级别定义"
            );
        }

        drop(lock_guard);
    }

    /// 计算定时任务当前触发标记
    fn try_get_schedule_trigger_key(&self, task: &task_setting::Model) -> Option<String> {
        let Some(cron_expression) = task
            .cron
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            tracing::warn!(
                target: "task_service.schedule.trigger",
                task_id = task.id,
                task_name = %task.name,
                "定时任务缺少 cron，已跳过"
            );
            return None;
        };

        let schedule = match Schedule::from_str(cron_expression) {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(
                    target: "task_service.schedule.trigger",
                    task_id = task.id,
                    task_name = %task.name,
                    cron = %cron_expression,
                    error = %error,
                    "定时任务 Cron 解析失败"
                );
                return None;
            }
        };

        let now = Utc::now();
        let lookback_milliseconds =
            Ord::max(self.options.schedule_poll_interval_milliseconds, 1) as i64;
        let window_start = now - chrono::Duration::milliseconds(lookback_milliseconds + 1000);

        let last_occurrence = schedule
            .after(&window_start)
            .next()
            .filter(|time| *time <= now)?;

        Some(format!("cron:{}", normalize_trigger_time(last_occurrence)))
    }

    /// 标记定时任务本机已触发的时间片
    ///
    /// 返回 true 表示当前时间片尚未执行过
    fn try_mark_schedule_trigger(&self, task_id: i64, trigger_key: &str) -> bool {
        let Ok(mut states) = self.schedule_trigger_states.lock() else {
            return true;
        };

        match states.get(&task_id) {
            Some(existing_key) if existing_key == trigger_key => false,
            _ => {
                states.insert(task_id, trigger_key.to_string());
                true
            }
        }
    }

    /// 在执行任务期间自动续锁
    async fn execute_with_auto_renew<F>(
        &self,
        future: F,
        lock_guard: &LockGuard,
        expiry_seconds: u64,
        task_id: i64,
        task_name: &str,
        is_queue_task: bool,
    ) -> Result<(), String>
    where
        F: Future<Output = Result<(), String>>,
    {
        let renew_interval_seconds = get_lock_renew_interval_seconds(expiry_seconds);
        let renew_expiry = StdDuration::from_secs(expiry_seconds);
        let mut renew_interval = time::interval_at(
            Instant::now() + Duration::from_secs(renew_interval_seconds),
            Duration::from_secs(renew_interval_seconds),
        );

        renew_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        tokio::pin!(future);

        loop {
            tokio::select! {
                result = &mut future => {
                    return result;
                }
                _ = renew_interval.tick() => {
                    match self.distributed_lock.renew(lock_guard, renew_expiry).await {
                        Ok(true) => {
                            if is_queue_task
                                && let Err(error) = self.renew_queue_task_lease(task_id).await
                            {
                                tracing::error!(
                                    target: "task_service.lock.renew",
                                    task_id = task_id,
                                    task_name = %task_name,
                                    error = %error,
                                    "队列任务租约回写失败"
                                );
                            }
                            tracing::debug!(
                                target: "task_service.lock.renew",
                                task_id = task_id,
                                task_name = %task_name,
                                "任务执行锁续期成功"
                            );
                        }
                        Ok(false) => {
                            tracing::warn!(
                                target: "task_service.lock.renew",
                                task_id = task_id,
                                task_name = %task_name,
                                "任务执行锁续期失败，锁可能已失效"
                            );
                        }
                        Err(error) => {
                            tracing::error!(
                                target: "task_service.lock.renew",
                                task_id = task_id,
                                task_name = %task_name,
                                error = %error,
                                "任务执行锁续期异常"
                            );
                        }
                    }
                }
            }
        }
    }

    /// 回写队列任务租约续期
    /// 仅更新当前实例已领取且仍处于执行中的任务
    async fn renew_queue_task_lease(&self, task_id: i64) -> Result<(), String> {
        let task_model = queue_task::Entity::find_by_id(task_id)
            .not_deleted(queue_task::Column::DeleteTime)
            .one(self.db.as_ref())
            .await
            .map_err(|error| format!("加载队列任务租约失败: {error}"))?;

        let Some(task_model) = task_model else {
            return Ok(());
        };

        if task_model.worker_id.as_deref() != Some(self.worker_id.as_str())
            || task_model.status != QueueTaskStatus::Running
            || task_model.success_time.is_some()
        {
            return Ok(());
        }

        let mut active = task_model.into_active_model();
        active.lease_expire_time = Set(Some(
            (Utc::now() + chrono::Duration::seconds(self.options.queue_lock_expiry_seconds as i64))
                .into(),
        ));

        active
            .update(self.db.as_ref())
            .await
            .map_err(|error| format!("更新队列任务租约失败: {error}"))?;

        Ok(())
    }

    /// 写入任务执行日志到数据库
    async fn write_execution_log(
        &self,
        level: &str,
        category: &str,
        content: String,
    ) -> Result<(), sea_orm::DbErr> {
        let log_id = self.identifier_generator.get_id().await.map_err(|error| {
            sea_orm::DbErr::Custom(format!("生成任务执行日志 ID 失败: {error}"))
        })?;

        log::ActiveModel {
            id: Set(log_id),
            project: Set(self.options.log_project.clone()),
            machine_name: Set(get_machine_name()),
            level: Set(level.to_string()),
            category: Set(category.to_string()),
            content: Set(content),
            create_time: Set(Utc::now().into()),
            delete_time: Set(None),
        }
        .insert(self.db.as_ref())
        .await
        .map(|_| ())
    }
}

/// 规范化触发时间文本
///
/// 用于生成稳定的定时任务触发标记
fn normalize_trigger_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

/// 获取锁续期间隔秒数
///
/// 默认按锁过期时长的一半续期，最小为一秒
fn get_lock_renew_interval_seconds(expiry_seconds: u64) -> u64 {
    let effective_expiry_seconds = Ord::max(expiry_seconds, 1);
    let renew_interval_seconds = effective_expiry_seconds / 2;

    Ord::max(renew_interval_seconds, 1)
}

/// 获取机器名称
fn get_machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// 创建当前任务宿主实例标识
/// 用于在任务表中标记任务归属和续租来源
fn create_worker_id() -> String {
    let worker_id = format!("{}-{}", get_machine_name(), std::process::id());
    worker_id.chars().take(48).collect()
}
