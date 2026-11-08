use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;

/// 功能授权实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "FunctionAuthorize")]
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

    /// 授权主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 功能 ID
    #[sea_orm(column_name = "FunctionId")]
    pub function_id: i64,

    /// 角色 ID
    #[sea_orm(column_name = "RoleId")]
    pub role_id: Option<i64>,

    /// 用户 ID
    #[sea_orm(column_name = "UserId")]
    pub user_id: Option<i64>,

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

/// 功能授权关联关系
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

    /// 关联角色
    #[sea_orm(
        belongs_to = "super::role::Entity",
        from = "Column::RoleId",
        to = "super::role::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    Role,

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

/// 提供到功能节点的关联
impl Related<super::function::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Function.def()
    }
}

/// 提供到角色的关联
impl Related<super::role::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Role.def()
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
        if !insert && !self.update_time.is_set() {
            self.update_time = Set(Some(Utc::now().into()));
        }

        Ok(self)
    }
}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
