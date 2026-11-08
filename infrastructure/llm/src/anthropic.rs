use crate::payload::Payload;
use async_trait::async_trait;
use futures_util::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use source_generator_core::ProtocolModel;

use crate::{
    LLM_REQUEST_TIMEOUT, LlmClient, LlmError, LlmModelConfig,
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
    pub fn new(http_client: reqwest::Client, config: LlmModelConfig) -> Self {
        Self {
            http_client,
            config,
        }
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
            .timeout(LLM_REQUEST_TIMEOUT)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .header(reqwest::header::CONTENT_TYPE, crate::JSON_CONTENT_TYPE)
            .send()
            .await?;

        let wire: AnthropicMessageResponseDto =
            crate::http_response::read_json(response, "Anthropic response is empty.").await?;
        map_response(wire, &request.model)
    }

    /// 对话（流式）
    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxStream<'_, Result<ChatStreamChunk, LlmError>> {
        Box::pin(async_stream::try_stream! {
            self.validate(&request)?;
            let body = build_payload(&request, true)?;

            let response = crate::streaming::send(self
                .http_client
                .post(self.config.endpoint.trim())
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .json(&body)
                .header(reqwest::header::CONTENT_TYPE, crate::JSON_CONTENT_TYPE)
            ).await?;

            crate::http_response::ensure_success(&response)?;

            let mut lines = crate::streaming::lines(response);
            let mut message_id: Option<String> = None;
            let mut model = request.model.clone();
            let mut input_tokens: Option<i32> = None;

            while let Some(line) = lines.next().await {
                let line = line?;
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }

                let wire: Option<AnthropicStreamEventDto> = crate::wire::from_slice(data.as_bytes())
                    .map_err(|_| LlmError::StreamParse("Anthropic stream returned invalid JSON.".into()))?;
                let wire = wire.ok_or_else(|| LlmError::StreamParse("Anthropic stream returned an empty JSON payload.".into()))?;

                match wire.event_type.as_deref().unwrap_or_default() {
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
                        let finish_reason = wire.delta.and_then(|delta| delta.stop_reason);
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
                                finish_reason,
                            }],
                            usage,
                        };
                    }
                    "message_stop" => { return; }
                    "error" => {
                        let error = wire.error;
                        let mut message = error.as_ref().and_then(|error| error.message.clone())
                            .unwrap_or_else(|| "Anthropic stream returned an error.".to_string());
                        if let Some(kind) = error.and_then(|error| error.error_type).filter(|kind| !kind.trim().is_empty()) {
                            message.push_str(&format!(" ({kind})"));
                        }
                        Err(LlmError::InvalidRequest(message))?;
                    }
                    _ => {}
                }
            }
        })
    }
}

/// 构造 Anthropic Messages 请求体
fn build_payload(request: &ChatRequest, stream: bool) -> Result<Payload, LlmError> {
    let mut payload = Payload::new();
    payload.insert("model".to_string(), Value::String(request.model.clone()));
    payload.insert("messages".to_string(), build_messages(&request.messages));
    payload.insert("stream".to_string(), Value::Bool(stream));

    let system_prompt = build_system_prompt(&request.messages);
    if !system_prompt.trim().is_empty() {
        payload.insert("system".to_string(), Value::String(system_prompt));
    }

    if let Some(extra_body) = &request.extra_body {
        payload.merge_extra(extra_body)?;
    }

    if !payload.contains_key("max_tokens") {
        payload.insert(
            "max_tokens".to_string(),
            Value::Number(DEFAULT_MAX_TOKENS.into()),
        );
    }

    Ok(payload)
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
fn map_response(
    dto: AnthropicMessageResponseDto,
    fallback_model: &str,
) -> Result<ChatResponse, LlmError> {
    let usage = dto.usage.map(|usage| Usage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens: add_tokens(usage.input_tokens, usage.output_tokens),
    });

    Ok(ChatResponse {
        id: dto.id,
        model: dto.model.unwrap_or_else(|| fallback_model.to_string()),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: ChatRole::Assistant,
                content: extract_text(&dto.content)?,
            },
            finish_reason: dto.stop_reason,
        }],
        usage,
    })
}

/// 提取 Anthropic 输出文本
fn extract_text(content: &Option<Vec<Option<AnthropicContentDto>>>) -> Result<String, LlmError> {
    let mut output = String::new();
    for item in content.as_deref().unwrap_or_default() {
        let item = crate::wire::require_object(item.as_ref())?;
        if item.content_type.as_deref() == Some("text") {
            output.push_str(item.text.as_deref().unwrap_or_default());
        }
    }
    Ok(output)
}

/// 合计 token
fn add_tokens(left: Option<i32>, right: Option<i32>) -> Option<i32> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.wrapping_add(right)),
        _ => None,
    }
}

/// Anthropic 非流式响应
#[derive(ProtocolModel)]
struct AnthropicMessageResponseDto {
    id: Option<String>,
    model: Option<String>,
    content: Option<Vec<Option<AnthropicContentDto>>>,
    #[serde(rename = "stop_reason")]
    stop_reason: Option<String>,
    usage: Option<AnthropicUsageDto>,
}

/// Anthropic 输出内容
#[derive(ProtocolModel)]
struct AnthropicContentDto {
    #[serde(rename = "type")]
    content_type: Option<String>,
    text: Option<String>,
}

/// Anthropic 流式事件
#[derive(ProtocolModel)]
struct AnthropicStreamEventDto {
    #[serde(rename = "type")]
    event_type: Option<String>,
    message: Option<AnthropicMessageResponseDto>,
    delta: Option<AnthropicDeltaDto>,
    usage: Option<AnthropicUsageDto>,
    error: Option<AnthropicErrorDto>,
}

/// Anthropic 流式增量
#[derive(ProtocolModel)]
struct AnthropicDeltaDto {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
    #[serde(rename = "stop_reason")]
    stop_reason: Option<String>,
}

/// Anthropic 用量统计
#[derive(ProtocolModel)]
struct AnthropicUsageDto {
    #[serde(rename = "input_tokens")]
    input_tokens: Option<i32>,
    #[serde(rename = "output_tokens")]
    output_tokens: Option<i32>,
}

/// Anthropic 错误详情
#[derive(ProtocolModel)]
struct AnthropicErrorDto {
    #[serde(rename = "type")]
    error_type: Option<String>,
    message: Option<String>,
}
