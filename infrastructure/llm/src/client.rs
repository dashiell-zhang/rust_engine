use async_trait::async_trait;
use futures_util::stream::BoxStream;

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
}
