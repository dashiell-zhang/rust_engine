use sea_orm::entity::prelude::*;

/// 日志实体
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "Log")]
pub struct Model {
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
