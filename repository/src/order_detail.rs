use sea_orm::entity::prelude::*;

/// 订单明细实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "OrderDetail")]
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

    /// 明细主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 订单 ID
    #[sea_orm(column_name = "OrderId")]
    pub order_id: i64,

    /// 商品 ID
    #[sea_orm(column_name = "ProductId")]
    pub product_id: i64,

    /// 购买数量
    #[sea_orm(column_name = "Number")]
    pub number: i32,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 订单明细关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联订单
    #[sea_orm(
        belongs_to = "super::order::Entity",
        from = "Column::OrderId",
        to = "super::order::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    Order,

    /// 关联商品
    #[sea_orm(
        belongs_to = "super::product::Entity",
        from = "Column::ProductId",
        to = "super::product::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    Product,
}

/// 提供到订单的关联
impl Related<super::order::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Order.def()
    }
}

/// 提供到商品的关联
impl Related<super::product::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Product.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
