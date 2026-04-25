use sea_orm::entity::prelude::*;

/// 队列任务执行状态
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "i32", db_type = "Integer")]
pub enum QueueTaskStatus {
    /// 待执行
    #[sea_orm(num_value = 0)]
    Pending,

    /// 执行中
    #[sea_orm(num_value = 1)]
    Running,

    /// 执行成功
    #[sea_orm(num_value = 2)]
    Succeeded,

    /// 执行失败
    #[sea_orm(num_value = 3)]
    Failed,
}

/// 队列任务实体
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "QueueTask")]
pub struct Model {
    /// 队列任务主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 任务名称
    #[sea_orm(column_name = "Name", column_type = "Text")]
    pub name: String,

    /// 任务参数
    #[sea_orm(column_name = "Parameter", column_type = "Text", nullable)]
    pub parameter: Option<String>,

    /// 执行状态
    #[sea_orm(column_name = "Status")]
    pub status: QueueTaskStatus,

    /// 重试次数
    #[sea_orm(column_name = "Count")]
    pub count: i32,

    /// 首次执行时间
    #[sea_orm(column_name = "FirstTime")]
    pub first_time: Option<DateTimeWithTimeZone>,

    /// 最近执行时间
    #[sea_orm(column_name = "LastTime")]
    pub last_time: Option<DateTimeWithTimeZone>,

    /// 成功完成时间
    #[sea_orm(column_name = "SuccessTime")]
    pub success_time: Option<DateTimeWithTimeZone>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,

    /// 计划执行时间
    #[sea_orm(column_name = "PlanTime")]
    pub plan_time: Option<DateTimeWithTimeZone>,

    /// 当前执行实例标识
    #[sea_orm(column_name = "WorkerId", column_type = "Text", nullable)]
    pub worker_id: Option<String>,

    /// 执行租约过期时间
    #[sea_orm(column_name = "LeaseExpireTime")]
    pub lease_expire_time: Option<DateTimeWithTimeZone>,

    /// 回调任务名称
    #[sea_orm(column_name = "CallbackName", column_type = "Text", nullable)]
    pub callback_name: Option<String>,

    /// 回调任务参数
    #[sea_orm(column_name = "CallbackParameter", column_type = "Text", nullable)]
    pub callback_parameter: Option<String>,

    /// 子任务全部成功时间
    #[sea_orm(column_name = "ChildSuccessTime")]
    pub child_success_time: Option<DateTimeWithTimeZone>,

    /// 父任务 ID
    #[sea_orm(column_name = "ParentTaskId")]
    pub parent_task_id: Option<i64>,
}

/// 队列任务关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}
