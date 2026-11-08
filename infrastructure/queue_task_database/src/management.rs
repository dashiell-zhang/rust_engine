//! 固定业务主库上的任务管理能力，不注册或启动 Worker。
use crate::{
    ARGS_DEFAULT_PARAMETER,
    queue_task::{self, QueueTaskStatus},
    task_setting,
};
use chrono::{DateTime, FixedOffset, Utc};
use common::{app_error::AppError, cron_helper};
use identifier_generator::IdentifierGenerator;
use repository_core::{SoftDeleteEntityExt, versioned::VersionedActiveModel};
use sea_orm::{
    ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, ExprTrait, IntoActiveModel,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait, sea_query::Expr,
};

type Result<T> = std::result::Result<T, AppError>;

pub struct TaskPage<T> {
    pub total: i32,
    pub list: Vec<T>,
}

pub struct QueueTaskQuery {
    pub skip: i32,
    pub take: i32,
    pub is_success: Option<bool>,
    pub start: Option<DateTime<FixedOffset>>,
    pub end: Option<DateTime<FixedOffset>>,
    pub parameter_keyword: Option<String>,
}

pub struct TaskSettingUpdate {
    pub parameter: Option<String>,
    pub semaphore: Option<i32>,
    pub cron: Option<String>,
    pub is_enable: bool,
    pub remark: Option<String>,
}

pub struct NewScheduleTask {
    pub name: String,
    pub parameter: String,
    pub cron: Option<String>,
    pub is_enable: bool,
    pub remark: Option<String>,
}

/// 操作前后快照供应用记录业务审计；组件不依赖业务日志或用户实体。
pub struct TaskSettingChange {
    pub before: task_setting::Model,
    pub after: task_setting::Model,
}

pub struct QueueTaskManagement<'a> {
    db: &'a DatabaseConnection,
    ids: &'a IdentifierGenerator,
}

impl<'a> QueueTaskManagement<'a> {
    pub fn new(db: &'a DatabaseConnection, ids: &'a IdentifierGenerator) -> Self {
        Self { db, ids }
    }

    pub async fn query_tasks(
        &self,
        request: QueueTaskQuery,
    ) -> Result<TaskPage<queue_task::Model>> {
        validate_page(request.skip, request.take)?;
        let mut query = queue_task::Entity::find_active();
        if let Some(success) = request.is_success {
            query = query.filter(if success {
                queue_task::Column::SuccessTime.is_not_null()
            } else {
                queue_task::Column::SuccessTime.is_null()
            });
        }
        if let Some(start) = request.start {
            query = query.filter(queue_task::Column::CreateTime.gte(start));
        }
        if let Some(end) = request.end {
            query = query.filter(queue_task::Column::CreateTime.lte(end));
        }
        if let Some(keyword) = request
            .parameter_keyword
            .filter(|value| !value.trim().is_empty())
        {
            // Contains 按原文查询；不能把用户输入的 %、_ 或反斜线解释为 SQL 通配符。
            let pattern = repository_core::extensions::contains_pattern(&keyword);
            query = query.filter(Expr::col(queue_task::Column::Parameter).like(pattern));
        }
        let total = query.clone().count(self.db).await.map_err(database_error)?;
        let total = repository_core::pagination::count_i32(total).map_err(database_error)?;
        let list = query
            .order_by_desc(queue_task::Column::Id)
            .offset(request.skip as u64)
            .limit(request.take as u64)
            .all(self.db)
            .await
            .map_err(database_error)?;
        Ok(TaskPage { total, list })
    }

    pub async fn query_settings(
        &self,
        skip: i32,
        take: i32,
        category: Option<&str>,
    ) -> Result<TaskPage<task_setting::Model>> {
        validate_page(skip, take)?;
        let mut query = task_setting::Entity::find_active();
        if let Some(category) = category.filter(|value| !value.trim().is_empty()) {
            query = query.filter(task_setting::Column::Category.eq(category));
        }
        let total = query.clone().count(self.db).await.map_err(database_error)?;
        let total = repository_core::pagination::count_i32(total).map_err(database_error)?;
        let list = query
            .order_by_desc(task_setting::Column::Id)
            .offset(skip as u64)
            .limit(take as u64)
            .all(self.db)
            .await
            .map_err(database_error)?;
        Ok(TaskPage { total, list })
    }

    pub async fn args_schedule_names(&self) -> Result<Vec<String>> {
        task_setting::Entity::find_active()
            .filter(task_setting::Column::Category.eq("ScheduleTask"))
            .filter(task_setting::Column::Parameter.eq(ARGS_DEFAULT_PARAMETER))
            .select_only()
            .column(task_setting::Column::Name)
            .distinct()
            .order_by_asc(task_setting::Column::Name)
            .into_tuple::<String>()
            .all(self.db)
            .await
            .map_err(database_error)
    }

    /// 在行锁内判断成功状态和执行租约，避免重试按钮与正在执行的 Worker 竞争。
    pub async fn retry(&self, id: i64) -> Result<()> {
        let tx = self.db.begin().await.map_err(database_error)?;
        let task = queue_task::Entity::find_by_id_active(id)
            .lock_exclusive()
            .one(&tx)
            .await
            .map_err(database_error)?
            .ok_or_else(|| AppError::bad_request("无效的 QueueTaskId"))?;
        if task.success_time.is_some() {
            return Err(AppError::bad_request("已成功的任务不允许重试"));
        }
        if task.status == QueueTaskStatus::Running
            && task.lease_expire_time.is_some_and(|time| time > Utc::now())
        {
            return Err(AppError::bad_request(
                "任务仍在执行，请等待执行结束或租约到期后重试",
            ));
        }
        let mut active = task.into_active_model();
        active.plan_time = Set(None);
        active.count = Set(0);
        active.first_time = Set(None);
        active.last_time = Set(None);
        active.child_success_time = Set(None);
        active.status = Set(QueueTaskStatus::Pending);
        active.worker_id = Set(None);
        active.lease_expire_time = Set(None);
        active.update_versioned(&tx).await.map_err(database_error)?;
        tx.commit().await.map_err(database_error)
    }

    pub async fn update_setting(
        &self,
        id: i64,
        actor_user_id: i64,
        request: TaskSettingUpdate,
    ) -> Result<TaskSettingChange> {
        let tx = self.db.begin().await.map_err(database_error)?;
        let result = self
            .update_setting_in_transaction(&tx, id, actor_user_id, request)
            .await?;
        tx.commit().await.map_err(database_error)?;
        Ok(result)
    }

    /// 调用方可以在同一事务中保存业务审计；此方法不提交事务。
    pub async fn update_setting_in_transaction(
        &self,
        tx: &DatabaseTransaction,
        id: i64,
        actor_user_id: i64,
        request: TaskSettingUpdate,
    ) -> Result<TaskSettingChange> {
        let before = task_setting::Entity::find_by_id_active(id)
            .one(tx)
            .await
            .map_err(database_error)?
            .ok_or_else(|| AppError::bad_request("无效的 taskSettingId"))?;
        if request.semaphore.is_some_and(|value| value <= 0) {
            return Err(AppError::bad_request("任务并发值必须大于零"));
        }
        let cron = if before.category == "ScheduleTask" {
            if before.parameter.is_none()
                || before.parameter.as_deref() == Some(ARGS_DEFAULT_PARAMETER)
            {
                if request.parameter != before.parameter {
                    return Err(AppError::bad_request(
                        "任务定义或参数模板不能修改为运行实例，请通过新增带参任务创建实例",
                    ));
                }
            } else {
                validate_parameter(request.parameter.as_deref())?;
            }
            normalize_cron(request.cron)?
        } else {
            request.cron
        };
        let mut active = before.clone().into_active_model();
        active.parameter = Set(request.parameter);
        active.semaphore = Set(request.semaphore);
        active.cron = Set(cron);
        active.is_enable = Set(request.is_enable);
        active.remark = Set(request.remark);
        active.update_user_id = Set(Some(actor_user_id));
        let after = active.update_versioned(tx).await.map_err(database_error)?;
        Ok(TaskSettingChange { before, after })
    }

    pub async fn create_schedule(
        &self,
        actor_user_id: i64,
        request: NewScheduleTask,
    ) -> Result<task_setting::Model> {
        let tx = self.db.begin().await.map_err(database_error)?;
        let result = self
            .create_schedule_in_transaction(&tx, actor_user_id, request)
            .await?;
        tx.commit().await.map_err(database_error)?;
        Ok(result)
    }

    pub async fn create_schedule_in_transaction(
        &self,
        tx: &DatabaseTransaction,
        actor_user_id: i64,
        request: NewScheduleTask,
    ) -> Result<task_setting::Model> {
        if request.name.trim().is_empty() {
            return Err(AppError::bad_request("任务名称不能为空"));
        }
        validate_parameter(Some(&request.parameter))?;
        let template = task_setting::Entity::find_active()
            .filter(task_setting::Column::Category.eq("ScheduleTask"))
            .filter(task_setting::Column::Name.eq(&request.name))
            .filter(task_setting::Column::Parameter.eq(ARGS_DEFAULT_PARAMETER))
            .one(tx)
            .await
            .map_err(database_error)?;
        if template.is_none() {
            return Err(AppError::bad_request(
                "带参任务模板不存在，请先确认 TaskService 已同步任务配置",
            ));
        }
        let cron = normalize_cron(request.cron)?;
        let id = self
            .ids
            .get_id()
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
        task_setting::ActiveModel {
            row_version: Default::default(),
            id: Set(id),
            category: Set("ScheduleTask".into()),
            name: Set(request.name),
            parameter: Set(Some(request.parameter)),
            semaphore: Set(None),
            cron: Set(cron),
            is_enable: Set(request.is_enable),
            remark: Set(request.remark),
            create_time: Set(Utc::now().into()),
            create_user_id: Set(Some(actor_user_id)),
            update_time: Set(None),
            update_user_id: Set(None),
            delete_time: Set(None),
            delete_user_id: Set(None),
        }
        .insert_versioned(tx)
        .await
        .map_err(database_error)
    }

    /// 保留任务中心现有的动态实例删除入口，定义和模板不能删除。
    pub async fn delete_setting_in_transaction(
        &self,
        tx: &DatabaseTransaction,
        id: i64,
        actor_user_id: i64,
    ) -> Result<TaskSettingChange> {
        let before = task_setting::Entity::find_by_id_active(id)
            .one(tx)
            .await
            .map_err(database_error)?
            .ok_or_else(|| AppError::bad_request("无效的 taskSettingId"))?;
        if before.category != "ScheduleTask"
            || before
                .parameter
                .as_deref()
                .is_none_or(|value| value.is_empty() || value == ARGS_DEFAULT_PARAMETER)
        {
            return Err(AppError::bad_request("仅允许删除动态创建的带参定时任务"));
        }
        let mut active = before.clone().into_active_model();
        active.is_enable = Set(false);
        active.delete_time = Set(Some(Utc::now().into()));
        active.delete_user_id = Set(Some(actor_user_id));
        active.update_user_id = Set(Some(actor_user_id));
        let after = active.update_versioned(tx).await.map_err(database_error)?;
        Ok(TaskSettingChange { before, after })
    }
}

fn validate_page(skip: i32, take: i32) -> Result<()> {
    if skip < 0 || !(1..=100).contains(&take) {
        return Err(AppError::internal(
            "分页偏移不能为负数，每页应为 1 至 100 条",
        ));
    }
    Ok(())
}
fn validate_parameter(parameter: Option<&str>) -> Result<()> {
    let parameter = parameter
        .filter(|value| !value.trim().is_empty() && *value != ARGS_DEFAULT_PARAMETER)
        .ok_or_else(|| AppError::bad_request("任务参数必须是合法 JSON"))?;
    serde_json::from_str::<serde_json::Value>(parameter)
        .map(|_| ())
        .map_err(|_| AppError::bad_request("任务参数必须是合法 JSON"))
}
fn normalize_cron(cron: Option<String>) -> Result<Option<String>> {
    let Some(cron) = cron.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let cron = cron.trim();
    cron_helper::validate(cron).map_err(|_| AppError::bad_request("Cron 表达式无效"))?;
    Ok(Some(cron.to_owned()))
}
fn database_error(error: DbErr) -> AppError {
    AppError::internal(error.to_string())
}
