use async_trait::async_trait;
use common::app_error::AppError;
pub use common::cancellation::CancellationToken;
use futures_util::stream::BoxStream;

pub use application_model::llm::invocation::*;

/// 应用层 LLM 客户端抽象
#[async_trait]
pub trait AppLlmClient: Send + Sync {
    /// 执行非流式对话
    async fn chat(&self, request: AppLlmChatRequest) -> Result<AppLlmChatResponse, AppError>;

    /// 执行流式对话
    fn chat_stream(
        &self,
        request: AppLlmChatRequest,
    ) -> BoxStream<'_, Result<AppLlmStreamChunk, AppError>>;

    async fn chat_with_cancellation(
        &self,
        request: AppLlmChatRequest,
        token: CancellationToken,
    ) -> Result<AppLlmChatResponse, AppError> {
        common::cancellation::run(&token, self.chat(request)).await
    }

    fn chat_stream_with_cancellation(
        &self,
        request: AppLlmChatRequest,
        token: CancellationToken,
    ) -> BoxStream<'_, Result<AppLlmStreamChunk, AppError>> {
        common::cancellation::stream(self.chat_stream(request), token)
    }
}
