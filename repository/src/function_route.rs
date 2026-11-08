use sea_orm::entity::prelude::*;

/// 功能路由绑定实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "FunctionRoute")]
pub struct Model {
    /// PostgreSQL 系统行版本；读取为无符号事务 ID 的文本，保存时只用于并发条件。
    #[sea_orm(
        column_name = "xmin",
        column_type = "Text",
        select_as = "text",
        save_as = "xid"
    )]
    pub row_version: String,

    /// 路由绑定主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 功能 ID
    #[sea_orm(column_name = "FunctionId")]
    pub function_id: Option<i64>,

    /// 模块名称
    #[sea_orm(column_name = "Module", column_type = "Text")]
    pub module: String,

    /// 路由地址
    #[sea_orm(column_name = "Route", column_type = "Text")]
    pub route: String,

    /// 路由备注
    #[sea_orm(column_name = "Remark", column_type = "Text", nullable)]
    pub remark: Option<String>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 功能路由绑定关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联功能节点
    #[sea_orm(
        belongs_to = "super::function::Entity",
        from = "Column::FunctionId",
        to = "super::function::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    Function,
}

/// 提供到功能节点的关联
impl Related<super::function::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Function.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
