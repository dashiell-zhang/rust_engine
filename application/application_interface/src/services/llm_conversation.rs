use application_model::llm::models::{LlmConversationPageListDto, LlmConversationPageRequest};
use async_trait::async_trait;
use common::app_error::AppError;

/// LLM 对话记录查询应用契约。
#[async_trait]
pub trait LlmConversationManageService: Send + Sync {
    async fn get_llm_conversation_list(
        &self,
        params: LlmConversationPageRequest,
    ) -> Result<LlmConversationPageListDto, AppError>;
}
