use std::time::Duration;

use async_trait::async_trait;
use futures_util::{StreamExt, stream::BoxStream};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    LlmClient, LlmError, LlmModelConfig,
    models::{
        ChatChoice, ChatMessage, ChatRequest, ChatResponse, ChatRole, ChatStreamChoiceDelta,
        ChatStreamChunk, ChatStreamDelta, Usage,
    },
};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: i32 = 4096;

/// Anthropic Messages API 协议客户端
pub struct AnthropicMessagesClient {
    /// 底层 HTTP 客户端
    http_client: reqwest::Client,

    /// 模型运行时配置
    config: LlmModelConfig,
}

impl AnthropicMessagesClient {
    /// 创建 Anthropic Messages API 客户端
    pub fn new(config: LlmModelConfig) -> Result<Self, LlmError> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;

        Ok(Self {
            http_client,
            config,
        })
    }

    /// 校验配置与请求
    fn validate(&self, request: &ChatRequest) -> Result<(), LlmError> {
        if self.config.endpoint.trim().is_empty() {
            return Err(LlmError::InvalidRequest(
                "Anthropic endpoint is required".to_string(),
            ));
        }

        if self.config.api_key.trim().is_empty() {
            return Err(LlmError::InvalidRequest(
                "Anthropic api key is required".to_string(),
            ));
        }

        if request.model.trim().is_empty() {
            return Err(LlmError::InvalidRequest("model is required".to_string()));
        }

        if request.messages.is_empty() {
            return Err(LlmError::InvalidRequest(
                "messages are required".to_string(),
            ));
        }

        Ok(())
    }
}

#[async_trait]
impl LlmClient for AnthropicMessagesClient {
    /// 对话（非流式）
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        self.validate(&request)?;
        let body = build_payload(&request, false)?;

        let response = self
            .http_client
            .post(self.config.endpoint.trim())
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError { status, message });
        }

        let wire: AnthropicMessageResponseDto = response.json().await?;
        Ok(map_response(wire, &request.model))
    }

    /// 对话（流式）
    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxStream<'_, Result<ChatStreamChunk, LlmError>> {
        Box::pin(async_stream::try_stream! {
            self.validate(&request)?;
            let body = build_payload(&request, true)?;

            let response = self
                .http_client
                .post(self.config.endpoint.trim())
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
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
            let mut buffer = String::new();
            let mut message_id: Option<String> = None;
            let mut model = request.model.clone();
            let mut input_tokens: Option<i32> = None;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| LlmError::Http(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim_end_matches('\r').to_string();
                    buffer = buffer[pos + 1..].to_string();

                    let Some(data) = line.strip_prefix("data:") else { continue };
                    let data = data.trim();
                    if data.is_empty() {
                        continue;
                    }

                    let wire: AnthropicStreamEventDto = serde_json::from_str(data)
                        .map_err(|e| LlmError::StreamParse(format!("无法解析 Anthropic SSE chunk: {e}, 原始数据: {data}")))?;

                    match wire.event_type.as_str() {
                        "message_start" => {
                            if let Some(message) = wire.message {
                                message_id = message.id;
                                model = message.model.unwrap_or_else(|| model.clone());
                                input_tokens = message.usage.and_then(|usage| usage.input_tokens);
                            }
                        }
                        "content_block_delta" => {
                            if let Some(delta) = wire.delta
                                && delta.delta_type.as_deref() == Some("text_delta")
                                && let Some(text) = delta.text
                                && !text.is_empty()
                            {
                                yield ChatStreamChunk {
                                    id: message_id.clone(),
                                    model: model.clone(),
                                    choices: vec![ChatStreamChoiceDelta {
                                        index: 0,
                                        delta: ChatStreamDelta {
                                            role: Some(ChatRole::Assistant),
                                            content: Some(text),
                                        },
                                        finish_reason: None,
                                    }],
                                    usage: None,
                                };
                            }
                        }
                        "message_delta" => {
                            let output_tokens = wire.usage.and_then(|usage| usage.output_tokens);
                            let usage = if input_tokens.is_none() && output_tokens.is_none() {
                                None
                            } else {
                                Some(Usage {
                                    prompt_tokens: input_tokens,
                                    completion_tokens: output_tokens,
                                    total_tokens: add_tokens(input_tokens, output_tokens),
                                })
                            };
                            yield ChatStreamChunk {
                                id: message_id.clone(),
                                model: model.clone(),
                                choices: vec![ChatStreamChoiceDelta {
                                    index: 0,
                                    delta: ChatStreamDelta {
                                        role: None,
                                        content: None,
                                    },
                                    finish_reason: wire.delta.and_then(|delta| delta.stop_reason),
                                }],
                                usage,
                            };
                        }
                        "message_stop" => {
                            return;
                        }
                        "error" => {
                            let message = wire
                                .error
                                .and_then(|error| error.message)
                                .unwrap_or_else(|| "Anthropic stream returned an error".to_string());
                            Err(LlmError::InvalidRequest(message))?;
                        }
                        _ => {}
                    }
                }
            }
        })
    }
}

/// 构造 Anthropic Messages 请求体
fn build_payload(request: &ChatRequest, stream: bool) -> Result<Map<String, Value>, LlmError> {
    let mut payload = Map::new();
    payload.insert("model".to_string(), Value::String(request.model.clone()));
    payload.insert("messages".to_string(), build_messages(&request.messages));
    payload.insert("stream".to_string(), Value::Bool(stream));

    let system_prompt = build_system_prompt(&request.messages);
    if !system_prompt.trim().is_empty() {
        payload.insert("system".to_string(), Value::String(system_prompt));
    }

    if let Some(temperature) = request.temperature
        && let Some(value) = serde_json::Number::from_f64(temperature)
    {
        payload.insert("temperature".to_string(), Value::Number(value));
    }

    if let Some(max_tokens) = request.max_tokens {
        payload.insert("max_tokens".to_string(), Value::Number(max_tokens.into()));
    }

    if let Some(extra_body) = &request.extra_body {
        merge_extra_body(&mut payload, extra_body)?;
    }

    if !payload.contains_key("max_tokens") {
        payload.insert(
            "max_tokens".to_string(),
            Value::Number(DEFAULT_MAX_TOKENS.into()),
        );
    }

    Ok(payload)
}

/// 合并额外请求参数
fn merge_extra_body(
    payload: &mut Map<String, Value>,
    extra_body: &Map<String, Value>,
) -> Result<(), LlmError> {
    for (key, value) in extra_body {
        if key.trim().is_empty() {
            continue;
        }

        if payload.contains_key(key) {
            return Err(LlmError::InvalidRequest(format!(
                "ExtraBody 与基础请求字段冲突: {key}"
            )));
        }

        payload.insert(key.clone(), value.clone());
    }

    Ok(())
}

/// 构造 Anthropic 系统提示词
fn build_system_prompt(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .filter(|message| message.role == ChatRole::System)
        .filter(|message| !message.content.trim().is_empty())
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// 构造 Anthropic 消息列表
fn build_messages(messages: &[ChatMessage]) -> Value {
    let messages = messages
        .iter()
        .filter(|message| message.role != ChatRole::System)
        .map(|message| {
            json!({
                "role": role_to_string(&message.role),
                "content": message.content,
            })
        })
        .collect::<Vec<_>>();

    if messages.is_empty() {
        Value::Array(vec![json!({"role": "user", "content": ""})])
    } else {
        Value::Array(messages)
    }
}

/// 转换角色
fn role_to_string(role: &ChatRole) -> &'static str {
    match role {
        ChatRole::Assistant => "assistant",
        _ => "user",
    }
}

/// 映射非流式响应
fn map_response(dto: AnthropicMessageResponseDto, fallback_model: &str) -> ChatResponse {
    let usage = dto.usage.map(|usage| Usage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens: add_tokens(usage.input_tokens, usage.output_tokens),
    });

    ChatResponse {
        id: dto.id,
        model: dto.model.unwrap_or_else(|| fallback_model.to_string()),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: ChatRole::Assistant,
                content: extract_text(&dto.content),
            },
            finish_reason: dto.stop_reason,
        }],
        usage,
    }
}

/// 提取 Anthropic 输出文本
fn extract_text(content: &Option<Vec<AnthropicContentDto>>) -> String {
    let mut output = String::new();
    for item in content.as_deref().unwrap_or_default() {
        if item.content_type.as_deref() == Some("text") {
            output.push_str(item.text.as_deref().unwrap_or_default());
        }
    }
    output
}

/// 合计 token
fn add_tokens(left: Option<i32>, right: Option<i32>) -> Option<i32> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + right),
        _ => None,
    }
}

/// Anthropic 非流式响应
#[derive(Deserialize)]
struct AnthropicMessageResponseDto {
    id: Option<String>,
    model: Option<String>,
    content: Option<Vec<AnthropicContentDto>>,
    #[serde(rename = "stop_reason")]
    stop_reason: Option<String>,
    usage: Option<AnthropicUsageDto>,
}

/// Anthropic 输出内容
#[derive(Deserialize)]
struct AnthropicContentDto {
    #[serde(rename = "type")]
    content_type: Option<String>,
    text: Option<String>,
}

/// Anthropic 流式事件
#[derive(Deserialize)]
struct AnthropicStreamEventDto {
    #[serde(rename = "type")]
    event_type: String,
    message: Option<AnthropicMessageResponseDto>,
    delta: Option<AnthropicDeltaDto>,
    usage: Option<AnthropicUsageDto>,
    error: Option<AnthropicErrorDto>,
}

/// Anthropic 流式增量
#[derive(Deserialize)]
struct AnthropicDeltaDto {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
    #[serde(rename = "stop_reason")]
    stop_reason: Option<String>,
}

/// Anthropic 用量统计
#[derive(Deserialize)]
struct AnthropicUsageDto {
    #[serde(rename = "input_tokens")]
    input_tokens: Option<i32>,
    #[serde(rename = "output_tokens")]
    output_tokens: Option<i32>,
}

/// Anthropic 错误详情
#[derive(Deserialize)]
struct AnthropicErrorDto {
    message: Option<String>,
}
