use sea_orm::entity::prelude::*;

/// 乡镇字典实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "RegionTown")]
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

    /// 乡镇主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i32,

    /// 乡镇名称
    #[sea_orm(column_name = "Town", column_type = "Text")]
    pub town: String,

    /// 区县 ID
    #[sea_orm(column_name = "AreaId")]
    pub area_id: i32,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 乡镇字典关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联区县
    #[sea_orm(
        belongs_to = "super::region_area::Entity",
        from = "Column::AreaId",
        to = "super::region_area::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    RegionArea,
}

/// 提供到区县的关联
impl Related<super::region_area::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RegionArea.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
