use sea_orm::entity::prelude::*;

/// LLM 会话记录实体
///
/// 记录每次 LLM 调用的系统提示词、用户输入与模型输出
#[repository_core::model_documentation]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "LlmConversation")]
pub struct Model {
    /// PostgreSQL 系统行版本；读取为无符号事务 ID 的文本，保存时只用于并发条件。
    #[sea_orm(
        column_name = "xmin",
        column_type = "Text",
        select_as = "text",
        save_as = "xid"
    )]
    pub row_version: String,

    /// 主键 ID
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 关联的 LLM 应用配置 ID
    #[sea_orm(column_name = "LlmAppId")]
    pub llm_app_id: i64,

    /// 系统提示词内容
    #[sea_orm(column_name = "SystemContent", column_type = "Text")]
    pub system_content: String,

    /// 用户消息内容
    #[sea_orm(column_name = "UserContent", column_type = "Text")]
    pub user_content: String,

    /// 助手回复内容
    #[sea_orm(column_name = "AssistantContent", column_type = "Text")]
    pub assistant_content: String,

    /// 创建人用户 ID
    #[sea_orm(column_name = "CreateUserId", nullable)]
    pub create_user_id: Option<i64>,

    /// 删除人用户 ID，与源 CD_User 基类一致。
    #[sea_orm(column_name = "DeleteUserId", nullable)]
    pub delete_user_id: Option<i64>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 软删除时间
    #[sea_orm(column_name = "DeleteTime", nullable)]
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// LLM 会话记录关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联 LLM 应用配置
    #[sea_orm(
        belongs_to = "super::llm_app::Entity",
        from = "Column::LlmAppId",
        to = "super::llm_app::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    LlmApp,

    /// 关联创建人用户
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreateUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    CreateUser,

    /// 关联删除人用户
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::DeleteUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    DeleteUser,
}

/// 提供到 LLM 应用配置的关联
impl Related<super::llm_app::Entity> for Entity {
    /// 返回关联定义
    fn to() -> RelationDef {
        Relation::LlmApp.def()
    }
}

/// 提供到创建人用户的关联
impl Related<super::user::Entity> for Entity {
    /// 返回关联定义
    fn to() -> RelationDef {
        Relation::CreateUser.def()
    }
}

/// 默认实体行为
impl ActiveModelBehavior for ActiveModel {}

repository_core::impl_versioned_entity!(Entity, Column::RowVersion);
