use sea_orm::entity::prelude::*;

/// 城市字典实体
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "RegionCity")]
pub struct Model {
    /// 城市主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i32,

    /// 城市名称
    #[sea_orm(column_name = "City", column_type = "Text")]
    pub city: String,

    /// 省份 ID
    #[sea_orm(column_name = "ProvinceId")]
    pub province_id: i32,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 城市字典关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联区县
    #[sea_orm(has_many = "super::region_area::Entity")]
    RegionArea,

    /// 关联省份
    #[sea_orm(
        belongs_to = "super::region_province::Entity",
        from = "Column::ProvinceId",
        to = "super::region_province::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    RegionProvince,
}

/// 提供到区县的关联
impl Related<super::region_area::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RegionArea.def()
    }
}

/// 提供到省份的关联
impl Related<super::region_province::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RegionProvince.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}
