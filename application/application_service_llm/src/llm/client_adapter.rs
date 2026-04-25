use async_trait::async_trait;
use futures_util::{StreamExt, stream::BoxStream};
use llm::{ChatMessage, ChatRequest, ChatRole, ChatStreamChunk, LlmClientFactory};

use application_interface::{
    error::AppError,
    llm::{
        AppLlmChatRequest, AppLlmChatResponse, AppLlmClient, AppLlmMessage, AppLlmRole,
        AppLlmStreamChunk, AppLlmUsage,
    },
};

/// 应用层 LLM 客户端适配器
///
/// 负责把应用层抽象请求转换为基础设施层请求
/// 并绑定固定 Provider 调用底层客户端
pub struct FactoryAppLlmClient<'a> {
    /// LLM 客户端工厂
    llm_client_factory: &'a LlmClientFactory,

    /// 绑定的 Provider 标识
    provider: String,
}

impl<'a> FactoryAppLlmClient<'a> {
    /// 创建应用层 LLM 客户端适配器
    ///
    /// 通过固定 Provider 把应用层抽象调用路由到底层客户端工厂
    pub fn new(llm_client_factory: &'a LlmClientFactory, provider: impl Into<String>) -> Self {
        Self {
            llm_client_factory,
            provider: provider.into(),
        }
    }

    /// 获取当前绑定的 Provider 标识
    pub fn provider(&self) -> &str {
        &self.provider
    }
}

#[async_trait]
impl AppLlmClient for FactoryAppLlmClient<'_> {
    /// 执行非流式对话
    async fn chat(&self, request: AppLlmChatRequest) -> Result<AppLlmChatResponse, AppError> {
        let client = self.require_client()?;
        let response = client
            .chat(to_chat_request(request))
            .await
            .map_err(|error| AppError::internal(format!("LLM 调用失败: {error}")))?;

        Ok(AppLlmChatResponse {
            response_id: response.id,
            model: response.model,
            messages: response.choices.into_iter().map(to_app_message).collect(),
            usage: response.usage.map(to_app_usage),
        })
    }

    /// 执行流式对话
    fn chat_stream(
        &self,
        request: AppLlmChatRequest,
    ) -> BoxStream<'_, Result<AppLlmStreamChunk, AppError>> {
        match self.require_client() {
            Ok(client) => Box::pin(async_stream::stream! {
                // 保持客户端引用存活到流消费结束
                let mut stream = client.chat_stream(to_chat_request(request));

                while let Some(item) = stream.next().await {
                    yield item.map(to_app_stream_chunk).map_err(to_stream_app_error);
                }
            }),
            Err(error) => Box::pin(futures_util::stream::once(async move { Err(error) })),
        }
    }
}

impl FactoryAppLlmClient<'_> {
    /// 获取当前 Provider 对应的基础设施客户端
    fn require_client(&self) -> Result<std::sync::Arc<dyn llm::LlmClient>, AppError> {
        self.llm_client_factory
            .require_client(&self.provider)
            .map_err(|error| AppError::internal(format!("获取 LLM 客户端失败: {error}")))
    }
}

/// 转换应用层请求为基础设施层请求
fn to_chat_request(request: AppLlmChatRequest) -> ChatRequest {
    let mut chat_request = ChatRequest::new(
        request.model,
        request.messages.into_iter().map(to_chat_message).collect(),
    );

    if let Some(temperature) = request.temperature {
        chat_request = chat_request.with_temperature(f64::from(temperature));
    }

    if let Some(max_tokens) = request
        .max_tokens
        .and_then(|value| i32::try_from(value).ok())
    {
        chat_request = chat_request.with_max_tokens(max_tokens);
    }

    if let Some(extra_body) = request.extra_body {
        // 透传应用层附带的 Provider 扩展参数
        chat_request = chat_request.with_extra_body(extra_body);
    }

    chat_request
}

/// 转换应用层消息为基础设施层消息
fn to_chat_message(message: AppLlmMessage) -> ChatMessage {
    ChatMessage {
        role: match message.role {
            AppLlmRole::System => ChatRole::System,
            AppLlmRole::User => ChatRole::User,
            AppLlmRole::Assistant => ChatRole::Assistant,
        },
        content: message.content,
    }
}

/// 转换基础设施层消息为应用层消息
fn to_app_message(choice: llm::ChatChoice) -> AppLlmMessage {
    AppLlmMessage {
        role: match choice.message.role {
            ChatRole::System => AppLlmRole::System,
            ChatRole::User => AppLlmRole::User,
            ChatRole::Assistant | ChatRole::Tool => AppLlmRole::Assistant,
        },
        content: choice.message.content,
    }
}

/// 转换基础设施层用量为应用层用量
fn to_app_usage(usage: llm::Usage) -> AppLlmUsage {
    AppLlmUsage {
        prompt_tokens: usage
            .prompt_tokens
            .and_then(|value| u32::try_from(value).ok()),
        completion_tokens: usage
            .completion_tokens
            .and_then(|value| u32::try_from(value).ok()),
        total_tokens: usage
            .total_tokens
            .and_then(|value| u32::try_from(value).ok()),
    }
}

/// 转换流式分片
fn to_app_stream_chunk(chunk: ChatStreamChunk) -> AppLlmStreamChunk {
    // 同一个分片内可能包含多个 choice 增量
    let delta = chunk
        .choices
        .iter()
        .filter_map(|choice| choice.delta.content.as_deref())
        .collect::<String>();

    let done = chunk
        .choices
        .iter()
        .any(|choice| choice.finish_reason.as_deref().is_some());

    AppLlmStreamChunk {
        delta: if delta.is_empty() { None } else { Some(delta) },
        done,
    }
}

/// 转换流式调用错误
fn to_stream_app_error(error: llm::LlmError) -> AppError {
    AppError::internal(format!("LLM 流式调用失败: {error}"))
}
