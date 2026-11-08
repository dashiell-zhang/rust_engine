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

/// OpenAI Responses API 协议客户端
pub struct OpenAiResponsesClient {
    /// 底层 HTTP 客户端
    http_client: reqwest::Client,

    /// 模型运行时配置
    config: LlmModelConfig,
}

impl OpenAiResponsesClient {
    /// 创建 Responses API 客户端
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
                "OpenAI Responses endpoint is required".to_string(),
            ));
        }

        if self.config.api_key.trim().is_empty() {
            return Err(LlmError::InvalidRequest(
                "OpenAI Responses api key is required".to_string(),
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
impl LlmClient for OpenAiResponsesClient {
    /// 对话（非流式）
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        self.validate(&request)?;
        let body = build_payload(&request, false)?;

        let response = self
            .http_client
            .post(self.config.endpoint.trim())
            .timeout(LLM_REQUEST_TIMEOUT)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .header(reqwest::header::CONTENT_TYPE, crate::JSON_CONTENT_TYPE)
            .send()
            .await?;

        let wire: ResponseDto =
            crate::http_response::read_json(response, "OpenAI Responses response is empty.")
                .await?;
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
                .bearer_auth(&self.config.api_key)
                .json(&body)
                .header(reqwest::header::CONTENT_TYPE, crate::JSON_CONTENT_TYPE)
            ).await?;

            crate::http_response::ensure_success(&response)?;

            let mut lines = crate::streaming::lines(response);

            while let Some(line) = lines.next().await {
                let line = line?;
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data.eq_ignore_ascii_case("[DONE]") { return; }
                if data.is_empty() {
                    continue;
                }

                let wire: Option<ResponseStreamEventDto> = crate::wire::from_slice(data.as_bytes())
                    .map_err(|_| LlmError::StreamParse("OpenAI Responses stream returned invalid JSON.".into()))?;
                let wire = wire.ok_or_else(|| LlmError::StreamParse("OpenAI Responses stream returned an empty JSON payload.".into()))?;

                if let Some(chunk) = map_stream_event(wire, &request.model)? {
                    yield chunk;
                }
            }
        })
    }
}

/// 构造 Responses API 请求体
fn build_payload(request: &ChatRequest, stream: bool) -> Result<Payload, LlmError> {
    let mut payload = Payload::new();
    payload.insert("model".to_string(), Value::String(request.model.clone()));
    payload.insert(
        "input".to_string(),
        Value::Array(
            request
                .messages
                .iter()
                .map(|message| {
                    json!({
                        "role": role_to_string(&message.role),
                        "content": message.content,
                    })
                })
                .collect(),
        ),
    );
    payload.insert("stream".to_string(), Value::Bool(stream));

    if let Some(user) = request.user.as_ref().filter(|user| !user.trim().is_empty()) {
        payload.insert("user".to_string(), Value::String(user.clone()));
    }

    if let Some(extra_body) = &request.extra_body {
        payload.merge_extra(extra_body)?;
    }

    Ok(payload)
}

/// 转换角色
fn role_to_string(role: &ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant | ChatRole::Tool => "assistant",
    }
}

/// 映射非流式响应
fn map_response(dto: ResponseDto, fallback_model: &str) -> Result<ChatResponse, LlmError> {
    let content = extract_output_text(&dto)?;
    let choice = ChatChoice {
        index: 0,
        message: ChatMessage {
            role: ChatRole::Assistant,
            content,
        },
        finish_reason: dto.status.clone(),
    };

    Ok(ChatResponse {
        id: dto.id,
        model: dto.model.unwrap_or_else(|| fallback_model.to_string()),
        choices: vec![choice],
        usage: dto.usage.map(ResponseUsageDto::into_usage),
    })
}

/// 映射流式事件
fn map_stream_event(
    dto: ResponseStreamEventDto,
    fallback_model: &str,
) -> Result<Option<ChatStreamChunk>, LlmError> {
    match dto.event_type.as_deref().unwrap_or_default() {
        "response.output_text.delta" => Ok(Some(ChatStreamChunk {
            id: dto.response_id,
            model: fallback_model.to_string(),
            choices: vec![ChatStreamChoiceDelta {
                index: 0,
                delta: ChatStreamDelta {
                    role: Some(ChatRole::Assistant),
                    content: dto.delta,
                },
                finish_reason: None,
            }],
            usage: None,
        })),
        "response.completed" => {
            let response = dto.response.ok_or_else(|| {
                LlmError::StreamParse(
                    "OpenAI Responses completed event is missing the response payload".to_string(),
                )
            })?;
            let usage = response.usage.clone().map(ResponseUsageDto::into_usage);
            Ok(Some(ChatStreamChunk {
                id: response.id.clone(),
                model: response
                    .model
                    .clone()
                    .unwrap_or_else(|| fallback_model.to_string()),
                choices: vec![ChatStreamChoiceDelta {
                    index: 0,
                    delta: ChatStreamDelta {
                        role: None,
                        content: None,
                    },
                    finish_reason: response.status,
                }],
                usage,
            }))
        }
        "response.failed" => {
            let message = dto
                .response
                .as_ref()
                .and_then(|item| item.error.as_ref())
                .and_then(|error| error.message.clone())
                .or_else(|| dto.error.as_ref().and_then(|error| error.message.clone()))
                .or(dto.message)
                .unwrap_or_else(|| "OpenAI Responses response failed".to_string());
            let code = dto
                .response
                .as_ref()
                .and_then(|item| item.error.as_ref())
                .and_then(|error| error.code.as_deref())
                .or_else(|| dto.error.as_ref().and_then(|error| error.code.as_deref()))
                .or(dto.code.as_deref());
            Err(LlmError::InvalidRequest(with_error_code(message, code)))
        }
        "response.incomplete" => {
            let reason = dto
                .response
                .and_then(|item| item.incomplete_details)
                .and_then(|details| details.reason)
                .filter(|reason| !reason.trim().is_empty());
            let message = reason.map_or_else(
                || "OpenAI Responses response is incomplete".to_string(),
                |reason| format!("OpenAI Responses response is incomplete: {reason}"),
            );
            Err(LlmError::InvalidRequest(message))
        }
        "error" => {
            let message = dto
                .message
                .or_else(|| dto.error.as_ref().and_then(|error| error.message.clone()))
                .unwrap_or_else(|| "OpenAI Responses stream returned an error".to_string());
            let code = dto
                .error
                .as_ref()
                .and_then(|error| error.code.as_deref())
                .or(dto.code.as_deref());
            Err(LlmError::InvalidRequest(with_error_code(message, code)))
        }
        _ => Ok(None),
    }
}

/// 在提供商错误消息后附加错误代码
fn with_error_code(message: String, code: Option<&str>) -> String {
    match code.filter(|value| !value.trim().is_empty()) {
        Some(code) => format!("{message} ({code})"),
        None => message,
    }
}

/// 提取输出文本
fn extract_output_text(dto: &ResponseDto) -> Result<String, LlmError> {
    if let Some(output_text) = &dto.output_text
        && !output_text.is_empty()
    {
        return Ok(output_text.clone());
    }

    let mut content = String::new();
    for item in dto.output.as_deref().unwrap_or_default() {
        let item = crate::wire::require_object(item.as_ref())?;
        for part in item.content.as_deref().unwrap_or_default() {
            let part = crate::wire::require_object(part.as_ref())?;
            if part.content_type.as_deref() == Some("output_text") {
                content.push_str(part.text.as_deref().unwrap_or_default());
            }
        }
    }

    Ok(content)
}

/// Responses API 非流式响应
#[derive(ProtocolModel)]
struct ResponseDto {
    id: Option<String>,
    model: Option<String>,
    status: Option<String>,
    #[serde(rename = "output_text")]
    output_text: Option<String>,
    output: Option<Vec<Option<ResponseOutputItemDto>>>,
    usage: Option<ResponseUsageDto>,
    error: Option<ResponseErrorDto>,
    #[serde(rename = "incomplete_details")]
    incomplete_details: Option<ResponseIncompleteDetailsDto>,
}

/// Responses API 输出项
#[derive(ProtocolModel)]
struct ResponseOutputItemDto {
    content: Option<Vec<Option<ResponseOutputContentDto>>>,
}

/// Responses API 输出内容
#[derive(ProtocolModel)]
struct ResponseOutputContentDto {
    #[serde(rename = "type")]
    content_type: Option<String>,
    text: Option<String>,
}

/// Responses API 流式事件
#[derive(ProtocolModel)]
struct ResponseStreamEventDto {
    #[serde(rename = "type")]
    event_type: Option<String>,
    #[serde(rename = "response_id")]
    response_id: Option<String>,
    delta: Option<String>,
    response: Option<ResponseDto>,
    message: Option<String>,
    error: Option<ResponseErrorDto>,
    code: Option<String>,
}

/// Responses API 错误详情
#[derive(ProtocolModel)]
struct ResponseErrorDto {
    code: Option<String>,
    message: Option<String>,
}

/// Responses API 响应不完整详情
#[derive(ProtocolModel)]
struct ResponseIncompleteDetailsDto {
    reason: Option<String>,
}

/// Responses API 用量统计
#[derive(Clone, ProtocolModel)]
struct ResponseUsageDto {
    #[serde(rename = "input_tokens")]
    input_tokens: Option<i32>,
    #[serde(rename = "output_tokens")]
    output_tokens: Option<i32>,
    #[serde(rename = "total_tokens")]
    total_tokens: Option<i32>,
}

impl ResponseUsageDto {
    /// 转换为统一用量模型
    fn into_usage(self) -> Usage {
        Usage {
            prompt_tokens: self.input_tokens,
            completion_tokens: self.output_tokens,
            total_tokens: self.total_tokens,
        }
    }
}
