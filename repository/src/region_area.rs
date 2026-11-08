use sea_orm::entity::prelude::*;

/// 区县字典实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "RegionArea")]
pub struct Model {
    /// PostgreSQL 系统行版本；读取为无符号事务 ID 的文本，保存时只用于并发条件。
    #[sea_orm(
        column_name = "xmin",
        column_type = "Text",
        select_as = "text",
        save_as = "xid"
    )]
    pub row_version: String,

    /// 区县主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i32,

    /// 区县名称
    #[sea_orm(column_name = "Area", column_type = "Text")]
    pub area: String,

    /// 城市 ID
    #[sea_orm(column_name = "CityId")]
    pub city_id: i32,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 区县字典关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联城市
    #[sea_orm(
        belongs_to = "super::region_city::Entity",
        from = "Column::CityId",
        to = "super::region_city::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    RegionCity,

    /// 关联乡镇
    #[sea_orm(has_many = "super::region_town::Entity")]
    RegionTown,

    /// 关联用户资料
    #[sea_orm(has_many = "super::user_info::Entity")]
    UserInfo,
}

/// 提供到城市的关联
impl Related<super::region_city::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RegionCity.def()
    }
}

/// 提供到乡镇的关联
impl Related<super::region_town::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RegionTown.def()
    }
}

/// 提供到用户资料的关联
impl Related<super::user_info::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserInfo.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
