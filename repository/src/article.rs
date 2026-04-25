use sea_orm::entity::prelude::*;

/// 文章实体
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "Article")]
pub struct Model {
    /// 文章主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 分类 ID
    #[sea_orm(column_name = "CategoryId")]
    pub category_id: i64,

    /// 标题
    #[sea_orm(column_name = "Title", column_type = "Text")]
    pub title: String,

    /// 正文内容
    #[sea_orm(column_name = "Content", column_type = "Text", nullable)]
    pub content: Option<String>,

    /// 是否推荐
    #[sea_orm(column_name = "IsRecommend")]
    pub is_recommend: bool,

    /// 是否显示
    #[sea_orm(column_name = "IsDisplay")]
    pub is_display: bool,

    /// 排序值
    #[sea_orm(column_name = "Sort")]
    pub sort: i32,

    /// 点击次数
    #[sea_orm(column_name = "ClickCount")]
    pub click_count: i32,

    /// 摘要
    #[sea_orm(column_name = "Digest", column_type = "Text", nullable)]
    pub digest: Option<String>,

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

/// 文章关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联分类
    #[sea_orm(
        belongs_to = "super::category::Entity",
        from = "Column::CategoryId",
        to = "super::category::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    Category,

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

/// 提供到分类的关联
impl Related<super::category::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Category.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}
