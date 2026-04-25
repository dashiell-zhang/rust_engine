use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;

/// LLM 应用配置实体
///
/// 存储各 LLM Provider 的业务配置与模型参数
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "LlmApp")]
pub struct Model {
    /// 主键 ID
    #[sea_orm(column_name = "Id", primary_key, auto_increment = false)]
    pub id: i64,

    /// 应用标记
    #[sea_orm(column_name = "Code", column_type = "Text")]
    pub code: String,

    /// 应用名称
    #[sea_orm(column_name = "Name", column_type = "Text")]
    pub name: String,

    /// Provider 标识，如 deepseek、qwen、openai
    #[sea_orm(column_name = "Provider", column_type = "Text")]
    pub provider: String,

    /// 模型名称，如 deepseek-chat、qwen-plus
    #[sea_orm(column_name = "Model", column_type = "Text")]
    pub model: String,

    /// System 提示词模板
    #[sea_orm(column_name = "SystemPromptTemplate", column_type = "Text", nullable)]
    pub system_prompt_template: Option<String>,

    /// User 提示词模板
    #[sea_orm(column_name = "PromptTemplate", column_type = "Text")]
    pub prompt_template: String,

    /// 最大生成 token 数
    #[sea_orm(column_name = "MaxTokens", nullable)]
    pub max_tokens: Option<i32>,

    /// 温度参数
    #[sea_orm(column_name = "Temperature", nullable)]
    pub temperature: Option<f64>,

    /// 额外请求参数 JSON 字符串
    #[sea_orm(column_name = "ExtraBodyJson", column_type = "Text", nullable)]
    pub extra_body_json: Option<String>,

    /// 是否启用
    #[sea_orm(column_name = "IsEnable")]
    pub is_enable: bool,

    /// 备注
    #[sea_orm(column_name = "Remark", column_type = "Text", nullable)]
    pub remark: Option<String>,

    /// 创建时间
    #[sea_orm(column_name = "CreateTime")]
    pub create_time: DateTimeWithTimeZone,

    /// 软删除时间
    #[sea_orm(column_name = "DeleteTime")]
    pub delete_time: Option<DateTimeWithTimeZone>,

    /// 更新时间
    #[sea_orm(column_name = "UpdateTime")]
    pub update_time: Option<DateTimeWithTimeZone>,

    /// 创建人 ID
    #[sea_orm(column_name = "CreateUserId")]
    pub create_user_id: Option<i64>,

    /// 更新人 ID
    #[sea_orm(column_name = "UpdateUserId")]
    pub update_user_id: Option<i64>,

    /// 删除人 ID
    #[sea_orm(column_name = "DeleteUserId")]
    pub delete_user_id: Option<i64>,
}

/// LLM 应用配置关联关系
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 关联创建人用户
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreateUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    CreateUser,

    /// 关联更新人用户
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UpdateUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    UpdateUser,

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
