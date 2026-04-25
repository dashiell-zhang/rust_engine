use sea_orm::entity::prelude::*;

/// 省份字典实体
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "RegionProvince")]
pub struct Model {
    /// 省份主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i32,

    /// 省份名称
    #[sea_orm(column_name = "Province", column_type = "Text")]
    pub province: String,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 省份字典关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联城市
    #[sea_orm(has_many = "super::region_city::Entity")]
    RegionCity,
}

/// 提供到城市的关联
impl Related<super::region_city::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RegionCity.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}
