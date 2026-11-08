use async_trait::async_trait;
use futures_util::{StreamExt, stream::BoxStream};
use llm::{ChatMessage, ChatRequest, ChatRole, ChatStreamChunk, LlmModelConfig, create_client};

use application_interface::{
    error::AppError,
    http_client::HttpClientFactory,
    llm::{
        AppLlmChatRequest, AppLlmChatResponse, AppLlmChoice, AppLlmClient, AppLlmMessage,
        AppLlmRole, AppLlmStreamChoice, AppLlmStreamChunk, AppLlmStreamDelta, AppLlmUsage,
    },
};

/// 基于单个模型配置的应用层 LLM 客户端适配器
pub struct ModelConfigAppLlmClient {
    /// 模型配置创建出的基础设施客户端
    client: std::sync::Arc<dyn llm::LlmClient>,
}

impl ModelConfigAppLlmClient {
    /// 创建模型配置客户端
    pub fn new(
        config: LlmModelConfig,
        http_client_factory: &HttpClientFactory,
    ) -> Result<Self, AppError> {
        let client = create_client(http_client_factory.streaming_client(), config)
            .map_err(|error| AppError::internal(format!("初始化 LLM 客户端失败: {error}")))?;

        Ok(Self { client })
    }
}

#[async_trait]
impl AppLlmClient for ModelConfigAppLlmClient {
    /// 执行非流式对话
    async fn chat(&self, request: AppLlmChatRequest) -> Result<AppLlmChatResponse, AppError> {
        let response = self
            .client
            .chat(to_chat_request(request))
            .await
            .map_err(to_app_error)?;

        Ok(AppLlmChatResponse {
            id: response.id,
            model: response.model,
            choices: response.choices.into_iter().map(to_app_choice).collect(),
            usage: response.usage.map(to_app_usage),
        })
    }

    /// 执行流式对话
    fn chat_stream(
        &self,
        request: AppLlmChatRequest,
    ) -> BoxStream<'_, Result<AppLlmStreamChunk, AppError>> {
        Box::pin(async_stream::stream! {
            let mut stream = self.client.chat_stream(to_chat_request(request));

            while let Some(item) = stream.next().await {
                yield item.map(to_app_stream_chunk).map_err(to_stream_app_error);
            }
        })
    }
}

/// 转换应用层请求为基础设施层请求
fn to_chat_request(request: AppLlmChatRequest) -> ChatRequest {
    let mut chat_request = ChatRequest::new(
        request.model,
        request.messages.into_iter().map(to_chat_message).collect(),
    );

    chat_request.user = request.user;

    if let Some(extra_body) = request.extra_body {
        // 透传应用层附带的模型协议扩展参数
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
            AppLlmRole::Tool => ChatRole::Tool,
        },
        content: message.content,
    }
}

fn to_app_role(role: ChatRole) -> AppLlmRole {
    match role {
        ChatRole::System => AppLlmRole::System,
        ChatRole::User => AppLlmRole::User,
        ChatRole::Assistant => AppLlmRole::Assistant,
        ChatRole::Tool => AppLlmRole::Tool,
    }
}

fn to_app_choice(choice: llm::ChatChoice) -> AppLlmChoice {
    AppLlmChoice {
        index: choice.index,
        message: AppLlmMessage {
            role: to_app_role(choice.message.role),
            content: choice.message.content,
        },
        finish_reason: choice.finish_reason,
    }
}

fn to_app_usage(usage: llm::Usage) -> AppLlmUsage {
    AppLlmUsage {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
    }
}

fn to_app_stream_chunk(chunk: ChatStreamChunk) -> AppLlmStreamChunk {
    AppLlmStreamChunk {
        id: chunk.id,
        model: chunk.model,
        usage: chunk.usage.map(to_app_usage),
        choices: chunk
            .choices
            .into_iter()
            .map(|choice| AppLlmStreamChoice {
                index: choice.index,
                finish_reason: choice.finish_reason,
                delta: AppLlmStreamDelta {
                    role: choice.delta.role.map(to_app_role),
                    content: choice.delta.content,
                },
            })
            .collect(),
    }
}

/// 转换流式调用错误
fn to_stream_app_error(error: llm::LlmError) -> AppError {
    match error {
        llm::LlmError::Cancelled => AppError::cancelled(),
        error => AppError::internal(format!("LLM 流式调用失败: {error}")),
    }
}

fn to_app_error(error: llm::LlmError) -> AppError {
    match error {
        llm::LlmError::Cancelled => AppError::cancelled(),
        error => AppError::internal(format!("LLM 调用失败: {error}")),
    }
}
