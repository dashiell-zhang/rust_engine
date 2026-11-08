use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;

/// 用户实体
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "User")]
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

    /// 用户主键
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 用户名
    #[sea_orm(column_name = "UserName", column_type = "Text")]
    pub user_name: String,

    /// 用户姓名；数据库比较遵循 citext，保留保存时的原始大小写。
    #[sea_orm(
        column_name = "Name",
        column_type = "custom(\"citext\")",
        save_as = "citext"
    )]
    pub name: String,

    /// 手机号
    #[sea_orm(column_name = "Phone", column_type = "Text")]
    pub phone: String,

    /// 邮箱
    #[sea_orm(column_name = "Email", column_type = "Text", nullable)]
    pub email: Option<String>,

    /// 密码散列值
    #[sea_orm(column_name = "Password", column_type = "Text")]
    pub password: String,

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

/// 用户关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联数据变更日志
    #[sea_orm(has_many = "super::data_update_log::Entity")]
    DataUpdateLog,

    /// 关联创建人
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::CreateUserId",
        to = "Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    CreateUser,

    /// 关联删除人
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::DeleteUserId",
        to = "Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    DeleteUser,

    /// 关联更新人
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::UpdateUserId",
        to = "Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    UpdateUser,

    /// 关联外部账号绑定
    #[sea_orm(has_many = "super::user_bind_external::Entity")]
    UserBindExternal,

    /// 用户资料与用户一对一，数据库唯一约束覆盖软删除资料。
    #[sea_orm(has_one = "super::user_info::Entity")]
    UserInfo,

    /// 关联用户 Token
    #[sea_orm(has_many = "super::user_token::Entity")]
    UserToken,
}

/// 提供到数据变更日志的关联
impl Related<super::data_update_log::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DataUpdateLog.def()
    }
}

/// 提供到外部账号绑定的关联
impl Related<super::user_bind_external::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserBindExternal.def()
    }
}

/// 提供到用户 Token 的关联
impl Related<super::user_token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserToken.def()
    }
}

/// 提供到用户资料的一对一关联。
impl Related<super::user_info::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserInfo.def()
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
