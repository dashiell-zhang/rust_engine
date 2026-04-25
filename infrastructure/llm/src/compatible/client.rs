use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::LlmClient;
use crate::error::LlmError;
use crate::models::{
    ChatChoice, ChatMessage, ChatRequest, ChatResponse, ChatRole, ChatStreamChoiceDelta,
    ChatStreamChunk, ChatStreamDelta, Usage,
};

use super::settings::OpenAiCompatibleSettings;

/// 基于 OpenAI 兼容协议的 LLM 客户端
pub struct OpenAiCompatibleClient {
    /// 底层 HTTP 客户端，复用连接池以减少握手开销
    http_client: reqwest::Client,

    /// Provider 连接配置，包含基址与密钥
    settings: OpenAiCompatibleSettings,
}

impl OpenAiCompatibleClient {
    /// 创建 OpenAI 兼容客户端
    ///
    /// 内部构建一个带超时配置的 `reqwest::Client`，失败时返回 `LlmError::Http`
    pub fn new(settings: OpenAiCompatibleSettings) -> Result<Self, LlmError> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;

        Ok(Self {
            http_client,
            settings,
        })
    }

    /// 拼接 chat completions 接口完整地址
    fn chat_url(&self) -> String {
        format!(
            "{}/chat/completions",
            self.settings.base_url.trim_end_matches('/')
        )
    }
}

#[async_trait]
impl LlmClient for OpenAiCompatibleClient {
    /// 对话（非流式），等待模型完整返回后统一解析响应体
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let body = WireRequestBody::from_request(&request, false);

        let response = self
            .http_client
            .post(self.chat_url())
            .bearer_auth(&self.settings.api_key)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError { status, message });
        }

        let wire: WireResponse = response.json().await?;

        Ok(wire.into_chat_response())
    }

    /// 对话（流式），逐行解析 SSE 响应并按分片 yield
    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxStream<'_, Result<ChatStreamChunk, LlmError>> {
        Box::pin(async_stream::try_stream! {
            let body = WireRequestBody::from_request(&request, true);

            let response = self.http_client
                .post(self.chat_url())
                .bearer_auth(&self.settings.api_key)
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let message = response.text().await.unwrap_or_default();
                Err(LlmError::ApiError { status, message })?;
                return;
            }

            let mut stream = response.bytes_stream();

            // 用于跨 chunk 拼接不完整行的缓冲区
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| LlmError::Http(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // 每次尽可能消费缓冲区中的完整行
                while let Some(pos) = buffer.find('\n') {

                    // 去掉行尾的 \r（兼容 \r\n 行尾）
                    let line = buffer[..pos].trim_end_matches('\r').to_string();
                    buffer = buffer[pos + 1..].to_string();

                    // SSE 数据行以 "data: " 开头
                    let Some(data) = line.strip_prefix("data: ") else { continue };
                    let data = data.trim();

                    // 流结束标记
                    if data == "[DONE]" {
                        return;
                    }

                    if data.is_empty() {
                        continue;
                    }

                    let wire: WireStreamChunk = serde_json::from_str(data)
                        .map_err(|e| LlmError::StreamParse(format!("无法解析 SSE chunk: {e}, 原始数据: {data}")))?;

                    yield wire.into_chat_stream_chunk();
                }
            }
        })
    }
}

// ===== 内部 Wire Format 类型 =====
// 以下类型仅用于与 OpenAI 兼容接口的 JSON 序列化/反序列化，不对外暴露

/// OpenAI 兼容接口的请求体
#[derive(Serialize)]
struct WireRequestBody {
    /// 模型名称
    model: String,

    /// 消息列表
    messages: Vec<WireMessage>,

    /// 温度参数，为 None 时不序列化，由 Provider 使用默认值
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,

    /// 最大生成 token 数，为 None 时不序列化
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,

    /// 是否启用流式返回
    stream: bool,

    /// 透传到根对象的额外参数
    #[serde(flatten)]
    extra_body: Map<String, Value>,
}

impl WireRequestBody {
    /// 从公开请求模型构建 wire 请求体
    fn from_request(request: &ChatRequest, stream: bool) -> Self {
        Self {
            model: request.model.clone(),
            messages: request
                .messages
                .iter()
                .map(WireMessage::from_chat_message)
                .collect(),
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream,
            extra_body: request.extra_body.clone().unwrap_or_default(),
        }
    }
}

/// 请求与非流式响应中通用的消息结构
#[derive(Serialize, Deserialize)]
struct WireMessage {
    /// 消息角色字符串，如 system / user / assistant
    role: String,

    /// 消息文本内容
    content: String,
}

impl WireMessage {
    /// 将公开消息模型转换为 wire 消息
    fn from_chat_message(msg: &ChatMessage) -> Self {
        let role = match msg.role {
            ChatRole::System => "system",
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool => "tool",
        };
        Self {
            role: role.to_string(),
            content: msg.content.clone(),
        }
    }

    /// 将 wire 消息转换为公开消息模型，未知 role 统一映射为 User
    fn into_chat_message(self) -> ChatMessage {
        let role = match self.role.as_str() {
            "system" => ChatRole::System,
            "assistant" => ChatRole::Assistant,
            "tool" => ChatRole::Tool,
            _ => ChatRole::User,
        };
        ChatMessage {
            role,
            content: self.content,
        }
    }
}

/// 响应中的 Token 用量统计
#[derive(Deserialize)]
struct WireUsage {
    /// 输入提示词消耗的 token 数
    prompt_tokens: Option<i32>,

    /// 模型生成内容消耗的 token 数
    completion_tokens: Option<i32>,

    /// 总 token 数
    total_tokens: Option<i32>,
}

impl WireUsage {
    /// 转换为公开用量模型
    fn into_usage(self) -> Usage {
        Usage {
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
        }
    }
}

/// 非流式响应中的单个候选结果
#[derive(Deserialize)]
struct WireChoice {
    /// 候选序号，通常为 0
    index: i32,

    /// 模型生成的消息
    message: WireMessage,

    /// 生成结束原因，如 stop / length
    finish_reason: Option<String>,
}

impl WireChoice {
    /// 转换为公开候选结果模型
    fn into_chat_choice(self) -> ChatChoice {
        ChatChoice {
            index: self.index,
            message: self.message.into_chat_message(),
            finish_reason: self.finish_reason,
        }
    }
}

/// 非流式响应体
#[derive(Deserialize)]
struct WireResponse {
    /// 请求/响应的唯一标识
    id: Option<String>,

    /// 实际使用的模型名称
    model: String,

    /// 候选结果列表
    choices: Vec<WireChoice>,

    /// Token 用量统计
    usage: Option<WireUsage>,
}

impl WireResponse {
    /// 转换为公开响应模型
    fn into_chat_response(self) -> ChatResponse {
        ChatResponse {
            id: self.id,
            model: self.model,
            choices: self
                .choices
                .into_iter()
                .map(|c| c.into_chat_choice())
                .collect(),
            usage: self.usage.map(|u| u.into_usage()),
        }
    }
}

/// 流式响应中的增量内容片段
#[derive(Deserialize)]
struct WireDelta {
    /// 部分 Provider 在首个 chunk 中携带 role 字段
    role: Option<String>,

    /// 本次新增的文本片段，末尾 chunk 通常为 None
    content: Option<String>,
}

impl WireDelta {
    /// 转换为公开流式增量模型，未知 role 统一映射为 User
    fn into_chat_stream_delta(self) -> ChatStreamDelta {
        let role = self.role.as_deref().map(|r| match r {
            "system" => ChatRole::System,
            "assistant" => ChatRole::Assistant,
            "tool" => ChatRole::Tool,
            _ => ChatRole::User,
        });
        ChatStreamDelta {
            role,
            content: self.content,
        }
    }
}

/// 流式响应中的单个候选增量
#[derive(Deserialize)]
struct WireStreamChoice {
    /// 候选序号，通常为 0
    index: i32,

    /// 本次增量内容
    delta: WireDelta,

    /// 生成结束原因，仅在流末尾 chunk 中出现
    finish_reason: Option<String>,
}

impl WireStreamChoice {
    /// 转换为公开流式候选增量模型
    fn into_chat_stream_choice_delta(self) -> ChatStreamChoiceDelta {
        ChatStreamChoiceDelta {
            index: self.index,
            delta: self.delta.into_chat_stream_delta(),
            finish_reason: self.finish_reason,
        }
    }
}

/// 流式响应的单个 SSE 分片体
#[derive(Deserialize)]
struct WireStreamChunk {
    /// 请求/响应的唯一标识
    id: Option<String>,

    /// 实际使用的模型名称
    model: String,

    /// 增量候选列表
    choices: Vec<WireStreamChoice>,

    /// 部分 Provider 在流末尾 chunk 中附带用量统计
    usage: Option<WireUsage>,
}

impl WireStreamChunk {
    /// 转换为公开流式分片模型
    fn into_chat_stream_chunk(self) -> ChatStreamChunk {
        ChatStreamChunk {
            id: self.id,
            model: self.model,
            choices: self
                .choices
                .into_iter()
                .map(|c| c.into_chat_stream_choice_delta())
                .collect(),
            usage: self.usage.map(|u| u.into_usage()),
        }
    }
}
