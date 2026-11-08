use application_model::llm::invocation::{AppLlmChatResponse, AppLlmStreamChunk};
use application_model::llm::parameters::LlmParameters;
use async_trait::async_trait;
use common::app_error::AppError;
use futures_util::stream::BoxStream;

#[async_trait]
impl application_interface::services::llm_invoke::LlmInvokeService
    for crate::llm::invoke_service::LlmInvokeService<'_>
{
    async fn chat(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
    ) -> Result<AppLlmChatResponse, AppError> {
        crate::llm::invoke_service::LlmInvokeService::chat(self, code, parameters, create_user_id)
            .await
    }
    async fn chat_content(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
    ) -> Result<Option<String>, AppError> {
        crate::llm::invoke_service::LlmInvokeService::chat_content(
            self,
            code,
            parameters,
            create_user_id,
        )
        .await
    }
    async fn chat_stream(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
    ) -> Result<BoxStream<'_, Result<AppLlmStreamChunk, AppError>>, AppError> {
        crate::llm::invoke_service::LlmInvokeService::chat_stream(
            self,
            code,
            parameters,
            create_user_id,
        )
        .await
    }
}
