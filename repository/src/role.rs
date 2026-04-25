use sea_orm::entity::prelude::*;

/// 角色实体
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "Role")]
pub struct Model {
    /// 角色主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 角色名称
    #[sea_orm(column_name = "Name", column_type = "Text")]
    pub name: String,

    /// 角色备注
    #[sea_orm(column_name = "Remark", column_type = "Text", nullable)]
    pub remark: Option<String>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,

    /// 角色编码
    #[sea_orm(column_name = "Code", column_type = "Text")]
    pub code: String,
}

/// 角色关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联功能授权
    #[sea_orm(has_many = "super::function_authorize::Entity")]
    FunctionAuthorize,

    /// 关联用户角色关系
    #[sea_orm(has_many = "super::user_role::Entity")]
    UserRole,
}

/// 提供到功能授权的关联
impl Related<super::function_authorize::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FunctionAuthorize.def()
    }
}

/// 提供到用户角色关系的关联
impl Related<super::user_role::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserRole.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}
