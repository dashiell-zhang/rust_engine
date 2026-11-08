use crate::{
    ARGS_DEFAULT_PARAMETER, NewQueueTask, QueueTaskProducer,
    queue_task::{self, Column, Entity, Model, QueueTaskStatus},
    task_setting,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use identifier_generator::IdentifierGenerator;
use repository_core::{SoftDeleteEntityExt, versioned::VersionedActiveModel};
use sea_orm::{
    ActiveEnum, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait,
    ExprTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
    sea_query::{Expr, LockType},
};
use std::{collections::HashSet, sync::Arc, time::Duration};

pub const MAX_EXECUTION_COUNT: i32 = 3;

/// 完成事务锁定记录并确认需要回调参数之后，才读取返回值。
pub type QueueTaskReturnValue = Box<dyn FnOnce() -> Result<Option<String>, DbErr> + Send>;

/// 模块声明的配置默认值；保存时始终禁用，不覆盖管理员已有配置。
pub struct TaskSettingDefinition {
    pub name: String,
    pub category: String,
    pub semaphore: Option<i32>,
    pub cron: Option<String>,
    pub remark: Option<String>,
    pub parameter: Option<String>,
}

/// 单个 Worker 所选任务库的持久化契约，不依赖任何具体业务仓库。
#[async_trait]
pub trait QueueTaskStore: Send + Sync {
    async fn get_settings(&self) -> Result<Vec<task_setting::Model>, DbErr>;
    async fn ensure_settings(&self, definitions: &[TaskSettingDefinition]) -> Result<(), DbErr>;
    async fn candidates(&self, name: &str, take: u64, running: &[i64]) -> Result<Vec<i64>, DbErr>;
    async fn try_claim(
        &self,
        id: i64,
        name: &str,
        worker: &str,
        lease: Duration,
    ) -> Result<bool, DbErr>;
    async fn release_claim(&self, id: i64, worker: &str) -> Result<(), DbErr>;
    async fn start_execution(&self, id: i64, worker: &str) -> Result<Option<Model>, DbErr>;
    async fn renew_lease(&self, id: i64, worker: &str, lease: Duration) -> Result<bool, DbErr>;
    async fn complete(
        &self,
        id: i64,
        worker: &str,
        return_value: QueueTaskReturnValue,
    ) -> Result<bool, DbErr>;
    async fn fail(&self, id: i64, worker: &str) -> Result<(), DbErr>;
}

pub struct DatabaseQueueTaskStore {
    db: Arc<DatabaseConnection>,
    ids: IdentifierGenerator,
}

impl DatabaseQueueTaskStore {
    pub fn new(db: Arc<DatabaseConnection>, ids: IdentifierGenerator) -> Self {
        Self { db, ids }
    }

    async fn has_definition(&self, definition: &TaskSettingDefinition) -> Result<bool, DbErr> {
        let query = task_setting::Entity::find_active()
            .filter(task_setting::Column::Category.eq(&definition.category))
            .filter(task_setting::Column::Name.eq(&definition.name));
        let query = if definition.category == "QueueTask" {
            query
        } else {
            query.filter(
                task_setting::Column::Parameter
                    .is_null()
                    .or(task_setting::Column::Parameter.eq(ARGS_DEFAULT_PARAMETER)),
            )
        };
        Ok(query.one(self.db.as_ref()).await?.is_some())
    }

    async fn owned(&self, id: i64, worker: &str) -> Result<Option<Model>, DbErr> {
        Entity::find_by_id_active(id)
            .filter(Column::WorkerId.eq(worker))
            .filter(Column::Status.eq(QueueTaskStatus::Running))
            .filter(Column::SuccessTime.is_null())
            .one(self.db.as_ref())
            .await
    }

    async fn release(&self, id: i64, worker: &str) -> Result<(), DbErr> {
        let Some(task) = self.owned(id, worker).await? else {
            return Ok(());
        };
        let status = if task.count >= MAX_EXECUTION_COUNT {
            QueueTaskStatus::Failed
        } else {
            QueueTaskStatus::Pending
        };
        let mut active = task.into_active_model();
        active.status = Set(status);
        active.lease_expire_time = Set(None);
        match active.update_versioned(self.db.as_ref()).await {
            Ok(_) | Err(DbErr::RecordNotUpdated) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[async_trait]
impl QueueTaskStore for DatabaseQueueTaskStore {
    async fn get_settings(&self) -> Result<Vec<task_setting::Model>, DbErr> {
        task_setting::Entity::find_active()
            .all(self.db.as_ref())
            .await
    }

    async fn ensure_settings(&self, definitions: &[TaskSettingDefinition]) -> Result<(), DbErr> {
        for definition in definitions {
            if self.has_definition(definition).await? {
                continue;
            }
            let id = self
                .ids
                .get_id()
                .await
                .map_err(|error| DbErr::Custom(error.to_string()))?;
            let result = task_setting::ActiveModel {
                id: Set(id),
                name: Set(definition.name.clone()),
                category: Set(definition.category.clone()),
                semaphore: Set(definition.semaphore),
                cron: Set(definition.cron.clone()),
                remark: Set(definition.remark.clone()),
                parameter: Set(definition.parameter.clone()),
                is_enable: Set(false),
                create_time: Set(Utc::now().into()),
                delete_time: Set(None),
                update_time: Set(None),
                create_user_id: Set(None),
                update_user_id: Set(None),
                delete_user_id: Set(None),
                row_version: Default::default(),
            }
            .insert_versioned(self.db.as_ref())
            .await;
            if let Err(error) = result {
                // 另一个 Worker 可能已提交相同定义；必须重新查询确认，不能吞掉其他失败。
                if !self.has_definition(definition).await? {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    async fn candidates(&self, name: &str, take: u64, running: &[i64]) -> Result<Vec<i64>, DbErr> {
        let now = Utc::now();
        // 最后一次执行中断后不再重试，也不能永久停留在 Running。
        Entity::update_many()
            .filter(Column::DeleteTime.is_null())
            .filter(Column::Name.eq(name))
            .filter(Column::Status.eq(QueueTaskStatus::Running))
            .filter(Column::SuccessTime.is_null())
            .filter(Column::Count.gte(MAX_EXECUTION_COUNT))
            .filter(Column::LeaseExpireTime.lt(now))
            .col_expr(
                Column::Status,
                Expr::value(QueueTaskStatus::Failed.to_value()),
            )
            .col_expr(Column::WorkerId, Expr::value(Option::<String>::None))
            .col_expr(
                Column::LeaseExpireTime,
                Expr::value(Option::<DateTime<Utc>>::None),
            )
            .exec(self.db.as_ref())
            .await?;
        let retry_due = Column::LastTime
            .is_null()
            .or(Expr::col(Column::LastTime).lt(Expr::value(now)
                .sub(Expr::col(Column::Count).mul(Expr::cust("INTERVAL '5 minutes'")))));
        Entity::find_active()
            .select_only()
            .column(Column::Id)
            .filter(Column::Name.eq(name))
            .filter(Column::CreateTime.lt(now - chrono::Duration::seconds(1)))
            .filter(Column::SuccessTime.is_null())
            .filter(Column::Count.lt(MAX_EXECUTION_COUNT))
            .filter(Column::PlanTime.is_null().or(Column::PlanTime.lte(now)))
            .filter(Column::Id.is_not_in(running.iter().copied()))
            .filter(
                Column::Status
                    .eq(QueueTaskStatus::Pending)
                    .and(retry_due)
                    .or(Column::Status
                        .eq(QueueTaskStatus::Running)
                        .and(Column::LeaseExpireTime.lt(now))),
            )
            .order_by_asc(Column::Count)
            .order_by_asc(Column::LastTime)
            .order_by_asc(Column::CreateTime)
            .limit(take)
            .into_tuple()
            .all(self.db.as_ref())
            .await
    }

    async fn try_claim(
        &self,
        id: i64,
        name: &str,
        worker: &str,
        lease: Duration,
    ) -> Result<bool, DbErr> {
        let now = Utc::now();
        let Some(task) = Entity::find_by_id_active(id).one(self.db.as_ref()).await? else {
            return Ok(false);
        };
        let retry_before = now
            .checked_sub_signed(chrono::Duration::minutes(5 * i64::from(task.count)))
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        let eligible = (task.status == QueueTaskStatus::Pending
            && task.last_time.is_none_or(|time| time < retry_before))
            || (task.status == QueueTaskStatus::Running
                && task.lease_expire_time.is_some_and(|time| time < now));
        if task.name != name
            || task.success_time.is_some()
            || task.count >= MAX_EXECUTION_COUNT
            || !eligible
            || task.plan_time.is_some_and(|time| time > now)
        {
            return Ok(false);
        }
        let mut active = task.into_active_model();
        active.status = Set(QueueTaskStatus::Running);
        active.worker_id = Set(Some(worker.to_owned()));
        active.lease_expire_time = Set(Some(lease_expiry(now, lease)?.into()));
        match active.update_versioned(self.db.as_ref()).await {
            Ok(_) => Ok(true),
            Err(DbErr::RecordNotUpdated) => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn release_claim(&self, id: i64, worker: &str) -> Result<(), DbErr> {
        self.release(id, worker).await
    }

    async fn start_execution(&self, id: i64, worker: &str) -> Result<Option<Model>, DbErr> {
        let Some(task) = self.owned(id, worker).await? else {
            return Ok(None);
        };
        let now = Utc::now().fixed_offset();
        let first = task.first_time.or(Some(now));
        let count = task
            .count
            .checked_add(1)
            .ok_or_else(|| DbErr::Custom("任务执行次数溢出".into()))?;
        let mut active = task.into_active_model();
        active.first_time = Set(first);
        active.last_time = Set(Some(now));
        active.count = Set(count);
        active.update_versioned(self.db.as_ref()).await.map(Some)
    }

    async fn renew_lease(&self, id: i64, worker: &str, lease: Duration) -> Result<bool, DbErr> {
        let Some(task) = self.owned(id, worker).await? else {
            return Ok(false);
        };
        let mut active = task.into_active_model();
        active.lease_expire_time = Set(Some(lease_expiry(Utc::now(), lease)?.into()));
        active.update_versioned(self.db.as_ref()).await?;
        Ok(true)
    }

    async fn complete(
        &self,
        id: i64,
        worker: &str,
        return_value: QueueTaskReturnValue,
    ) -> Result<bool, DbErr> {
        let transaction = self.db.begin().await?;
        let Some(task) = lock_task(&transaction, id).await? else {
            return Ok(false);
        };
        if task.worker_id.as_deref() != Some(worker) || task.status != QueueTaskStatus::Running {
            return Ok(false);
        }
        let mut active = task.clone().into_active_model();
        active.status = Set(QueueTaskStatus::Succeeded);
        active.success_time = Set(Some(Utc::now().into()));
        active.lease_expire_time = Set(None);
        if task.callback_name.is_some() && task.callback_parameter.is_none() {
            active.callback_parameter = Set(return_value()?);
        }
        let mut task = active.update_versioned(&transaction).await?;
        let producer = QueueTaskProducer::new(self.db.as_ref(), &self.ids);
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(task.id) {
                return Err(DbErr::Custom("任务父子链包含循环".into()));
            }
            if task.status != QueueTaskStatus::Succeeded
                || task.success_time.is_none()
                || task.child_success_time.is_some()
            {
                break;
            }
            let unfinished = Entity::find_active()
                .filter(Column::ParentTaskId.eq(task.id))
                .filter(Column::ChildSuccessTime.is_null())
                .count(&transaction)
                .await?;
            if unfinished > 0 {
                break;
            }
            let mut active = task.clone().into_active_model();
            active.child_success_time = Set(Some(Utc::now().into()));
            active.update_versioned(&transaction).await?;
            if let Some(name) = task.callback_name {
                producer
                    .create_internal(
                        &transaction,
                        NewQueueTask {
                            name,
                            parameter: task.callback_parameter,
                            ..Default::default()
                        },
                    )
                    .await?;
            }
            let Some(parent) = task.parent_task_id else {
                break;
            };
            task = lock_task(&transaction, parent)
                .await?
                .ok_or_else(|| DbErr::Custom("任务的父记录不存在".into()))?;
        }
        transaction.commit().await?;
        Ok(true)
    }

    async fn fail(&self, id: i64, worker: &str) -> Result<(), DbErr> {
        self.release(id, worker).await
    }
}

async fn lock_task(transaction: &DatabaseTransaction, id: i64) -> Result<Option<Model>, DbErr> {
    // 完成流程不修改主键。子记录在同一事务中多次更新会再次检查外键并持有父记录
    // KEY SHARE；父链使用 NO KEY UPDATE 仍串行化状态写入，同时避免兄弟事务
    // 各持 KEY SHARE 后再升级 FOR UPDATE 造成死锁。
    queue_task::Entity::find_by_id_active(id)
        .lock(LockType::NoKeyUpdate)
        .one(transaction)
        .await
}

fn lease_expiry(now: DateTime<Utc>, lease: Duration) -> Result<DateTime<Utc>, DbErr> {
    chrono::Duration::from_std(lease)
        .ok()
        .and_then(|lease| now.checked_add_signed(lease))
        .ok_or_else(|| DbErr::Custom("队列任务租约时长超出范围".into()))
}
