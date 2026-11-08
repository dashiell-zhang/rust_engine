use chrono::{DateTime, FixedOffset, Utc};
use identifier_generator::IdentifierGenerator;
use repository_core::versioned::VersionedActiveModel;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, Set};

use crate::queue_task::{ActiveModel, QueueTaskStatus};

/// 待入队任务；参数是调用方已序列化的文本，组件不依赖业务 DTO。
#[derive(Default)]
pub struct NewQueueTask {
    pub name: String,
    pub parameter: Option<String>,
    pub plan_time: Option<DateTime<FixedOffset>>,
    pub callback_name: Option<String>,
    pub callback_parameter: Option<String>,
    pub parent_task_id: Option<i64>,
}

impl NewQueueTask {
    /// 与 NetEngine 入队入口保持一致的公共校验。
    pub fn validate(&self) -> Result<(), DbErr> {
        if self.name.trim().is_empty() {
            return Err(DbErr::Custom("队列任务名称不可以为空".to_string()));
        }
        if self.callback_parameter.is_some()
            && self
                .callback_name
                .as_deref()
                .is_none_or(|name| name.trim().is_empty())
        {
            return Err(DbErr::Custom("回调参数需要指定有效回调名称".to_string()));
        }
        if self.plan_time.is_some_and(|time| time < Utc::now()) {
            return Err(DbErr::Custom("计划时间不能早于当前时间".to_string()));
        }
        Ok(())
    }
}

/// 绑定所选业务数据库的生产者，不保存或提交调用方的事务。
pub struct QueueTaskProducer<'a> {
    db: &'a DatabaseConnection,
    identifier_generator: &'a IdentifierGenerator,
}

impl<'a> QueueTaskProducer<'a> {
    pub fn new(db: &'a DatabaseConnection, identifier_generator: &'a IdentifierGenerator) -> Self {
        Self {
            db,
            identifier_generator,
        }
    }

    pub fn from_module<M: repository_core::module::DatabaseModule>(
        module: &'a repository_core::module::ModuleDatabase<M>,
        identifiers: &'a IdentifierGenerator,
    ) -> Self {
        Self::new(module.write(), identifiers)
    }

    /// 在调用方显式事务中入队，任务与业务写入一起提交或回滚。
    pub async fn create<M: repository_core::module::DatabaseModule>(
        &self,
        transaction: &repository_core::module::ModuleTransaction<M>,
        task: NewQueueTask,
    ) -> Result<i64, DbErr> {
        if !transaction.belongs_to(self.db) {
            return Err(DbErr::Custom(
                "入队事务不属于生产者绑定的数据库模块连接".into(),
            ));
        }
        self.insert(transaction.connection(), task).await
    }

    /// 队列存储内部完成事务已经由同一存储连接创建，不向应用暴露原始事务入口。
    pub(crate) async fn create_internal(
        &self,
        transaction: &sea_orm::DatabaseTransaction,
        task: NewQueueTask,
    ) -> Result<i64, DbErr> {
        self.insert(transaction, task).await
    }

    /// 使用绑定的连接池独立入队，失败直接报告给调用方。
    pub async fn create_single(&self, task: NewQueueTask) -> Result<i64, DbErr> {
        self.insert(self.db, task).await
    }

    async fn insert<C: ConnectionTrait + sea_orm::TransactionTrait>(
        &self,
        db: &C,
        task: NewQueueTask,
    ) -> Result<i64, DbErr> {
        task.validate()?;
        let id = self
            .identifier_generator
            .get_id()
            .await
            .map_err(|error| DbErr::Custom(format!("生成队列任务主键失败: {error}")))?;
        ActiveModel {
            row_version: Default::default(),
            id: Set(id),
            name: Set(task.name),
            parameter: Set(task.parameter),
            status: Set(QueueTaskStatus::Pending),
            count: Set(0),
            first_time: Set(None),
            last_time: Set(None),
            success_time: Set(None),
            create_time: Set(Utc::now().into()),
            delete_time: Set(None),
            plan_time: Set(task.plan_time),
            worker_id: Set(None),
            lease_expire_time: Set(None),
            callback_name: Set(task.callback_name),
            callback_parameter: Set(task.callback_parameter),
            child_success_time: Set(None),
            parent_task_id: Set(task.parent_task_id),
        }
        .insert_versioned(db)
        .await?;
        Ok(id)
    }
}
