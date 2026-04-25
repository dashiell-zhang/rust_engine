use sea_orm::entity::prelude::*;

/// 应用配置实体
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "AppSetting")]
pub struct Model {
    /// 配置主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 配置模块
    #[sea_orm(column_name = "Module", column_type = "Text")]
    pub module: String,

    /// 配置分组 ID
    #[sea_orm(column_name = "GroupId")]
    pub group_id: Option<i64>,

    /// 配置键
    #[sea_orm(column_name = "Key", column_type = "Text")]
    pub key: String,

    /// 配置值
    #[sea_orm(column_name = "Value", column_type = "Text")]
    pub value: String,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 应用配置关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}
