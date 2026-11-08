use sea_orm::entity::prelude::*;

/// 文章分类实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "Category")]
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

    /// 分类主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 分类名称
    #[sea_orm(column_name = "Name", column_type = "Text")]
    pub name: String,

    /// 排序值
    #[sea_orm(column_name = "Sort")]
    pub sort: i32,

    /// 父级分类 ID
    #[sea_orm(column_name = "ParentId")]
    pub parent_id: Option<i64>,

    /// 分类备注
    #[sea_orm(column_name = "Remark", column_type = "Text", nullable)]
    pub remark: Option<String>,

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
}

/// 文章分类关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联文章
    #[sea_orm(has_many = "super::article::Entity")]
    Article,

    /// 关联父级分类
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::ParentId",
        to = "Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    Parent,

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

/// 提供到文章的关联
impl Related<super::article::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Article.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
