use sea_orm::entity::prelude::*;

/// 文件记录实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "StoredFile")]
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

    /// 文件主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 文件名称
    #[sea_orm(column_name = "Name", column_type = "Text")]
    pub name: String,

    /// 文件相对路径
    #[sea_orm(column_name = "Path", column_type = "Text")]
    pub path: String,

    /// 所属业务表名
    #[sea_orm(column_name = "Table", column_type = "Text")]
    pub table: String,

    /// 所属业务主键
    #[sea_orm(column_name = "TableId")]
    pub table_id: Option<i64>,

    /// 文件标记
    #[sea_orm(column_name = "Sign", column_type = "Text")]
    pub sign: String,

    /// 排序值
    #[sea_orm(column_name = "Sort")]
    pub sort: i32,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,

    /// 创建人用户 ID
    #[sea_orm(column_name = "CreateUserId")]
    pub create_user_id: Option<i64>,

    /// 删除人用户 ID
    #[sea_orm(column_name = "DeleteUserId")]
    pub delete_user_id: Option<i64>,

    /// 文件长度
    #[sea_orm(column_name = "Length")]
    pub length: i64,

    /// 是否允许公开读取
    #[sea_orm(column_name = "IsPublicRead")]
    pub is_public_read: bool,
}

/// 文件记录关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联创建人
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreateUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    CreateUser,

    /// 关联删除人
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::DeleteUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    DeleteUser,
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
