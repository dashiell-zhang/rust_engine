use async_trait::async_trait;
use futures_util::stream::BoxStream;
use llm::{
    ChatRequest, ChatResponse, ChatStreamChunk, LlmClient, LlmError, OpenAiCompatibleClient,
    OpenAiCompatibleSettings,
};

use crate::settings::QwenSettings;

/// Qwen Provider 标识
pub const QWEN_PROVIDER: &str = "qwen";

/// Qwen 客户端
pub struct QwenClient {
    /// 基于 OpenAI Compatible 协议的底层客户端
    inner: OpenAiCompatibleClient,
}

impl QwenClient {
    /// 创建 Qwen 客户端
    pub fn new(settings: QwenSettings) -> Result<Self, LlmError> {
        let settings = OpenAiCompatibleSettings::new(settings.base_url, settings.api_key);
        let inner = OpenAiCompatibleClient::new(settings)?;

        Ok(Self { inner })
    }
}

#[async_trait]
impl LlmClient for QwenClient {
    /// 执行非流式对话
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        self.inner.chat(request).await
    }

    /// 执行流式对话
    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxStream<'_, Result<ChatStreamChunk, LlmError>> {
        self.inner.chat_stream(request)
    }
}
