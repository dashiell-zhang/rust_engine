use sea_orm::entity::prelude::*;

/// 用户 Token 实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "UserToken")]
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

    /// Token 主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 用户 ID
    #[sea_orm(column_name = "UserId")]
    pub user_id: i64,

    /// 最后访问标识
    #[sea_orm(column_name = "LastId")]
    pub last_id: Option<i64>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 用户 Token 关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联用户
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    User,
}

/// 提供到用户的关联
impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
