use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;

/// 定时任务配置实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "TaskSetting")]
#[derive(repository_core::SoftDelete)]
pub struct Model {
    /// PostgreSQL 系统行版本；读取为无符号事务 ID 的文本，保存时只用于并发条件。
    #[sea_orm(
        column_name = "xmin",
        column_type = "Text",
        select_as = "text",
        save_as = "xid"
    )]
    pub row_version: String,

    /// 任务配置主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 任务名称
    #[sea_orm(column_name = "Name", column_type = "Text")]
    pub name: String,

    /// 任务分类
    #[sea_orm(column_name = "Category", column_type = "Text")]
    pub category: String,

    /// 并发信号量
    #[sea_orm(column_name = "Semaphore")]
    pub semaphore: Option<i32>,

    /// Cron 表达式
    #[sea_orm(column_name = "Cron", column_type = "Text", nullable)]
    pub cron: Option<String>,

    /// 备注
    #[sea_orm(column_name = "Remark", column_type = "Text", nullable)]
    pub remark: Option<String>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,

    /// 更新时间
    #[sea_orm(column_name = "UpdateTime")]
    pub update_time: Option<DateTimeWithTimeZone>,

    /// 创建人用户 ID
    #[sea_orm(column_name = "CreateUserId")]
    pub create_user_id: Option<i64>,

    /// 更新人用户 ID
    #[sea_orm(column_name = "UpdateUserId")]
    pub update_user_id: Option<i64>,

    /// 删除人用户 ID
    #[sea_orm(column_name = "DeleteUserId")]
    pub delete_user_id: Option<i64>,

    /// 是否启用
    #[sea_orm(column_name = "IsEnable")]
    pub is_enable: bool,

    /// 任务参数
    #[sea_orm(column_name = "Parameter", column_type = "Text", nullable)]
    pub parameter: Option<String>,
}

/// 定时任务配置关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// 默认实体行为
#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 持久化前补齐更新时间
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert && !self.update_time.is_set() {
            self.update_time = Set(Some(Utc::now().into()));
        }

        Ok(self)
    }
}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
