use sea_orm::entity::prelude::*;

/// 日志实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, source_generator_core::PartitionTable)]
#[sea_orm(table_name = "Log")]
#[partition(interval = 1, unit = "Month")]
pub struct Model {
    /// PostgreSQL 系统行版本；读取为无符号事务 ID 的文本，保存时只用于并发条件。
    #[sea_orm(
        column_name = "xmin",
        column_type = "Text",
        select_as = "text",
        save_as = "xid"
    )]
    pub row_version: String,

    /// 日志主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 项目标识
    #[sea_orm(column_name = "Project", column_type = "Text")]
    pub project: String,

    /// 机器名称
    #[sea_orm(column_name = "MachineName", column_type = "Text")]
    pub machine_name: String,

    /// 日志级别
    #[sea_orm(column_name = "Level", column_type = "Text")]
    pub level: String,

    /// 日志分类
    #[sea_orm(column_name = "Category", column_type = "Text")]
    pub category: String,

    /// 日志内容
    #[sea_orm(column_name = "Content", column_type = "Text")]
    pub content: String,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 日志关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

impl repository_core::SoftDeleteEntityExt for Entity {
    fn delete_time_col() -> Column {
        Column::DeleteTime
    }
}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
