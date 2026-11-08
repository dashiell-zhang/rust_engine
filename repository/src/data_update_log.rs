use sea_orm::entity::prelude::*;

/// 数据变更日志实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "DataUpdateLog")]
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

    /// 日志主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 业务表名
    #[sea_orm(column_name = "Table", column_type = "Text")]
    pub table: String,

    /// 业务主键
    #[sea_orm(column_name = "TableId")]
    pub table_id: i64,

    /// 变更内容
    #[sea_orm(column_name = "Content", column_type = "Text")]
    pub content: String,

    /// 操作人用户 ID
    #[sea_orm(column_name = "ActionUserId")]
    pub action_user_id: Option<i64>,

    /// IP 地址
    #[sea_orm(column_name = "IpAddress", column_type = "Text", nullable)]
    pub ip_address: Option<String>,

    /// 设备标识
    #[sea_orm(column_name = "DeviceMark", column_type = "Text", nullable)]
    pub device_mark: Option<String>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 数据变更日志关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联操作人
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::ActionUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    ActionUser,
}

/// 提供到用户的关联
impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ActionUser.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
