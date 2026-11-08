use application_model::llm::invocation::{AppLlmChatResponse, AppLlmStreamChunk};
use application_model::llm::parameters::LlmParameters;
use async_trait::async_trait;
use common::app_error::AppError;
use common::cancellation::{self, CancellationToken};
use futures_util::stream::BoxStream;

/// LLM 对话与流式调用应用契约。
#[async_trait]
pub trait LlmInvokeService: Send + Sync {
    async fn chat(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
    ) -> Result<AppLlmChatResponse, AppError>;
    async fn chat_content(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
    ) -> Result<Option<String>, AppError>;
    async fn chat_stream(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
    ) -> Result<BoxStream<'_, Result<AppLlmStreamChunk, AppError>>, AppError>;

    /// 令牌覆盖配置查询、提供商调用和最终会话保存。
    async fn chat_with_cancellation(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
        token: CancellationToken,
    ) -> Result<AppLlmChatResponse, AppError> {
        cancellation::run(&token, self.chat(code, parameters, create_user_id)).await
    }

    async fn chat_content_with_cancellation(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
        token: CancellationToken,
    ) -> Result<Option<String>, AppError> {
        cancellation::run(&token, self.chat_content(code, parameters, create_user_id)).await
    }

    async fn chat_stream_with_cancellation(
        &self,
        code: &str,
        parameters: LlmParameters,
        create_user_id: Option<i64>,
        token: CancellationToken,
    ) -> Result<BoxStream<'_, Result<AppLlmStreamChunk, AppError>>, AppError> {
        let stream =
            cancellation::run(&token, self.chat_stream(code, parameters, create_user_id)).await?;
        Ok(cancellation::stream(stream, token))
    }
}
