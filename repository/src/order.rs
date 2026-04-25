use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;

/// 订单实体
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "Order")]
pub struct Model {
    /// 订单主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 订单号
    #[sea_orm(column_name = "OrderNo", column_type = "Text")]
    pub order_no: String,

    /// 订单类型
    #[sea_orm(column_name = "Type", column_type = "Text")]
    pub r#type: String,

    /// 订单金额
    #[sea_orm(column_name = "Price", column_type = "Decimal(Some((38, 2)))")]
    pub price: Decimal,

    /// 外部流水号
    #[sea_orm(column_name = "SerialNo", column_type = "Text", nullable)]
    pub serial_no: Option<String>,

    /// 订单状态
    #[sea_orm(column_name = "State", column_type = "Text")]
    pub state: String,

    /// 支付方式
    #[sea_orm(column_name = "PayType", column_type = "Text")]
    pub pay_type: String,

    /// 是否已支付
    #[sea_orm(column_name = "PayState")]
    pub pay_state: bool,

    /// 支付时间
    #[sea_orm(column_name = "PayTime")]
    pub pay_time: Option<DateTimeWithTimeZone>,

    /// 实付金额
    #[sea_orm(
        column_name = "PayPrice",
        column_type = "Decimal(Some((38, 2)))",
        nullable
    )]
    pub pay_price: Option<Decimal>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,

    /// 更新时间
    #[sea_orm(column_name = "UpdateTime")]
    pub update_time: Option<DateTimeWithTimeZone>,

    /// 创建人用户 ID
    #[sea_orm(column_name = "CreateUserId")]
    pub create_user_id: Option<i64>,

    /// 更新人用户 ID
    #[sea_orm(column_name = "UpdateUserId")]
    pub update_user_id: Option<i64>,

    /// 删除人用户 ID
    #[sea_orm(column_name = "DeleteUserId")]
    pub delete_user_id: Option<i64>,
}

/// 订单关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联订单明细
    #[sea_orm(has_many = "super::order_detail::Entity")]
    OrderDetail,

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

    /// 关联更新人
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UpdateUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    UpdateUser,
}

/// 提供到订单明细的关联
impl Related<super::order_detail::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OrderDetail.def()
    }
}

/// 默认实体行为
#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    /// 持久化前补齐更新时间
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert {
            self.update_time = Set(Some(Utc::now().into()));
        }

        Ok(self)
    }
}
