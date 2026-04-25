use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;

/// 用户资料实体
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "UserInfo")]
pub struct Model {
    /// 用户资料主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 用户 ID
    #[sea_orm(column_name = "UserId")]
    pub user_id: i64,

    /// 区县 ID
    #[sea_orm(column_name = "RegionAreaId")]
    pub region_area_id: Option<i32>,

    /// 地址
    #[sea_orm(column_name = "Address", column_type = "Text", nullable)]
    pub address: Option<String>,

    /// 性别
    #[sea_orm(column_name = "Sex")]
    pub sex: Option<bool>,

    /// 更新人用户 ID
    #[sea_orm(column_name = "UpdateUserId")]
    pub update_user_id: Option<i64>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,

    /// 更新时间
    #[sea_orm(column_name = "UpdateTime")]
    pub update_time: Option<DateTimeWithTimeZone>,
}

/// 用户资料关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联区县
    #[sea_orm(
        belongs_to = "super::region_area::Entity",
        from = "Column::RegionAreaId",
        to = "super::region_area::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    RegionArea,

    /// 关联更新人
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UpdateUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    UpdateUser,

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

/// 提供到区县的关联
impl Related<super::region_area::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RegionArea.def()
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
