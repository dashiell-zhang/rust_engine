use application_model::llm::models::{LlmConversationPageListDto, LlmConversationPageRequest};
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::llm_conversation::LlmConversationManageService
    for crate::llm::conversation_manage_service::LlmConversationManageService<'_>
{
    async fn get_llm_conversation_list(
        &self,
        params: LlmConversationPageRequest,
    ) -> Result<LlmConversationPageListDto, AppError> {
        crate::llm::conversation_manage_service::LlmConversationManageService::get_llm_conversation_list(self, params).await
    }
}
