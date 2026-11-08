use repository::extensions::SoftDeleteQueryExt;
use repository::{llm_app, llm_conversation, user};
use repository_core::pagination::SignedPage;
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, ExprTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use application_model::shared::models::{page_offset, validate_page_request};
use application_model::user::models::PageListDto;
use common::app_error::AppError;
use repository_core::ReadDatabaseConnection;

use super::models::{LlmConversationDto, LlmConversationPageListDto, LlmConversationPageRequest};

/// LLM 对话记录管理服务
///
/// 提供对话历史记录的分页查询能力
pub struct LlmConversationManageService<'a> {
    /// 宿主提供的业务只读连接
    read_db: &'a ReadDatabaseConnection,
}

impl<'a> LlmConversationManageService<'a> {
    /// 创建 LLM 对话记录管理服务
    ///
    /// 对话及关联信息使用同一个业务只读连接。
    pub fn new(read_db: &'a ReadDatabaseConnection) -> Self {
        Self { read_db }
    }

    /// 获取 LLM 对话记录分页列表
    pub async fn get_llm_conversation_list(
        &self,
        params: LlmConversationPageRequest,
    ) -> Result<LlmConversationPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query =
            llm_conversation::Entity::find().not_deleted(llm_conversation::Column::DeleteTime);

        if let Some(keyword) = params
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let pattern = repository_core::extensions::contains_pattern(keyword);
            // 先命中关联应用 再回查会话表 以对齐按应用编码和名称检索的语义
            let matched_llm_app_ids = llm_app::Entity::find()
                .not_deleted(llm_app::Column::DeleteTime)
                .filter(
                    llm_app::Column::Code
                        .like(pattern.clone())
                        .or(llm_app::Column::Name.like(pattern.clone())),
                )
                .select_only()
                .column(llm_app::Column::Id)
                .into_tuple::<i64>()
                .all(self.read_db)
                .await
                .map_err(|error| AppError::internal(format!("查询 LLM 应用配置失败: {error}")))?;

            query = query.filter(
                Condition::any()
                    .add(llm_conversation::Column::LlmAppId.is_in(matched_llm_app_ids))
                    .add(llm_conversation::Column::SystemContent.like(pattern.clone()))
                    .add(llm_conversation::Column::UserContent.like(pattern.clone()))
                    .add(llm_conversation::Column::AssistantContent.like(pattern)),
            );
        }

        if let Some(start_time) = params.start_time {
            query = query.filter(llm_conversation::Column::CreateTime.gte(start_time));
        }

        if let Some(end_time) = params.end_time {
            query = query.filter(llm_conversation::Column::CreateTime.lte(end_time));
        }

        let total = query
            .clone()
            .count(self.read_db)
            .await
            .map_err(|error| AppError::internal(format!("查询对话记录总数失败: {error}")))?;
        let total = repository_core::pagination::count_i32(total)
            .map_err(|error| AppError::internal(error.to_string()))?;

        if total == 0 {
            return Ok(PageListDto {
                total: 0,
                list: None,
            });
        }

        let skip = page_offset(params.page, params.page_size);
        let conversations = query
            .order_by_desc(llm_conversation::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.read_db)
            .await
            .map_err(|error| AppError::internal(format!("查询对话记录列表失败: {error}")))?;

        let llm_app_ids = conversations
            .iter()
            .map(|item| item.llm_app_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let llm_apps = if llm_app_ids.is_empty() {
            vec![]
        } else {
            llm_app::Entity::find()
                .filter(llm_app::Column::Id.is_in(llm_app_ids))
                .not_deleted(llm_app::Column::DeleteTime)
                .all(self.read_db)
                .await
                .map_err(|error| AppError::internal(format!("查询 LLM 应用配置失败: {error}")))?
        };

        let create_user_ids = conversations
            .iter()
            .filter_map(|item| item.create_user_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let users = if create_user_ids.is_empty() {
            vec![]
        } else {
            user::Entity::find()
                .filter(user::Column::Id.is_in(create_user_ids))
                .not_deleted(user::Column::DeleteTime)
                .all(self.read_db)
                .await
                .map_err(|error| AppError::internal(format!("查询创建人用户失败: {error}")))?
        };

        let list = conversations
            .into_iter()
            .map(|item| {
                let app = llm_apps
                    .iter()
                    .find(|llm_app| llm_app.id == item.llm_app_id);
                let create_user = item
                    .create_user_id
                    .and_then(|create_user_id| users.iter().find(|user| user.id == create_user_id));

                LlmConversationDto {
                    id: item.id.to_string(),
                    llm_app_id: item.llm_app_id.to_string(),
                    llm_app_code: app.map(|llm_app| llm_app.code.clone()).unwrap_or_default(),
                    llm_app_name: app.map(|llm_app| llm_app.name.clone()).unwrap_or_default(),
                    system_content: item.system_content,
                    user_content: item.user_content,
                    assistant_content: item.assistant_content,
                    create_user_id: item.create_user_id.map(|id| id.to_string()),
                    create_user_name: create_user.map(|user| user.name.clone()),
                    create_time: item.create_time.to_rfc3339(),
                }
            })
            .collect();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }
}
