use crate::CancellationToken;
use async_trait::async_trait;
use futures_util::{StreamExt, stream::BoxStream};

use crate::error::LlmError;
use crate::models::{ChatRequest, ChatResponse, ChatStreamChunk};

/// LLM 客户端统一接口
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// 对话（非流式），等待模型完整返回后再解析
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;

    /// 对话（流式），返回 SSE 分片流
    fn chat_stream(&self, request: ChatRequest)
    -> BoxStream<'_, Result<ChatStreamChunk, LlmError>>;

    /// 带取消令牌的完整调用；释放请求 future 会关闭尚未完成的响应读取。
    async fn chat_with_cancellation(
        &self,
        request: ChatRequest,
        token: CancellationToken,
    ) -> Result<ChatResponse, LlmError> {
        token
            .run_until_cancelled(self.chat(request))
            .await
            .ok_or(LlmError::Cancelled)?
    }

    /// 令牌覆盖响应头等待和后续逐片消费，取消不能作为正常流结束。
    fn chat_stream_with_cancellation(
        &self,
        request: ChatRequest,
        token: CancellationToken,
    ) -> BoxStream<'_, Result<ChatStreamChunk, LlmError>> {
        Box::pin(futures_util::stream::try_unfold(
            (self.chat_stream(request), token),
            |(mut stream, token)| async move {
                match token.run_until_cancelled(stream.next()).await {
                    Some(Some(item)) => Ok(Some((item?, (stream, token)))),
                    Some(None) => Ok(None),
                    None => Err(LlmError::Cancelled),
                }
            },
        ))
    }
}
