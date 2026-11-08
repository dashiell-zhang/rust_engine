use sea_orm::entity::prelude::*;

/// 功能节点实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "Function")]
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

    /// 功能主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 功能名称
    #[sea_orm(column_name = "Name", column_type = "Text")]
    pub name: String,

    /// 功能标识
    #[sea_orm(column_name = "Sign", column_type = "Text")]
    pub sign: String,

    /// 功能备注
    #[sea_orm(column_name = "Remark", column_type = "Text", nullable)]
    pub remark: Option<String>,

    /// 父节点 ID
    #[sea_orm(column_name = "ParentId")]
    pub parent_id: Option<i64>,

    /// 节点类型
    #[sea_orm(column_name = "Type")]
    pub r#type: i32,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 功能节点关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联父节点
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::ParentId",
        to = "Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    Parent,

    /// 关联功能授权
    #[sea_orm(has_many = "super::function_authorize::Entity")]
    FunctionAuthorize,

    /// 关联路由绑定
    #[sea_orm(has_many = "super::function_route::Entity")]
    FunctionRoute,
}

/// 提供到功能授权的关联
impl Related<super::function_authorize::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FunctionAuthorize.def()
    }
}

/// 提供到路由绑定的关联
impl Related<super::function_route::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FunctionRoute.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
