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

/// OpenAI Responses API 协议客户端
pub struct OpenAiResponsesClient {
    /// 底层 HTTP 客户端
    http_client: reqwest::Client,

    /// 模型运行时配置
    config: LlmModelConfig,
}

impl OpenAiResponsesClient {
    /// 创建 Responses API 客户端
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
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError { status, message });
        }

        let wire: ResponseDto = response.json().await?;
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
                .bearer_auth(&self.config.api_key)
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

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| LlmError::Http(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim_end_matches('\r').to_string();
                    buffer = buffer[pos + 1..].to_string();

                    let Some(data) = line.strip_prefix("data:") else { continue };
                    let data = data.trim();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }

                    let wire: ResponseStreamEventDto = serde_json::from_str(data)
                        .map_err(|e| LlmError::StreamParse(format!("无法解析 Responses SSE chunk: {e}, 原始数据: {data}")))?;

                    if let Some(chunk) = map_stream_event(wire, &request.model)? {
                        yield chunk;
                    }
                }
            }
        })
    }
}

/// 构造 Responses API 请求体
fn build_payload(request: &ChatRequest, stream: bool) -> Result<Map<String, Value>, LlmError> {
    let mut payload = Map::new();
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

    if let Some(temperature) = request.temperature
        && let Some(value) = serde_json::Number::from_f64(temperature)
    {
        payload.insert("temperature".to_string(), Value::Number(value));
    }

    if let Some(max_tokens) = request.max_tokens {
        payload.insert(
            "max_output_tokens".to_string(),
            Value::Number(max_tokens.into()),
        );
    }

    if let Some(extra_body) = &request.extra_body {
        merge_extra_body(&mut payload, extra_body)?;
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

/// 转换角色
fn role_to_string(role: &ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant | ChatRole::Tool => "assistant",
    }
}

/// 映射非流式响应
fn map_response(dto: ResponseDto, fallback_model: &str) -> ChatResponse {
    let content = extract_output_text(&dto);
    let choice = ChatChoice {
        index: 0,
        message: ChatMessage {
            role: ChatRole::Assistant,
            content,
        },
        finish_reason: dto.status.clone(),
    };

    ChatResponse {
        id: dto.id,
        model: dto.model.unwrap_or_else(|| fallback_model.to_string()),
        choices: vec![choice],
        usage: dto.usage.map(ResponseUsageDto::into_usage),
    }
}

/// 映射流式事件
fn map_stream_event(
    dto: ResponseStreamEventDto,
    fallback_model: &str,
) -> Result<Option<ChatStreamChunk>, LlmError> {
    match dto.event_type.as_str() {
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
            let response = dto.response;
            let usage = response
                .as_ref()
                .and_then(|item| item.usage.clone())
                .map(ResponseUsageDto::into_usage);
            Ok(Some(ChatStreamChunk {
                id: response.as_ref().and_then(|item| item.id.clone()),
                model: response
                    .as_ref()
                    .and_then(|item| item.model.clone())
                    .unwrap_or_else(|| fallback_model.to_string()),
                choices: vec![ChatStreamChoiceDelta {
                    index: 0,
                    delta: ChatStreamDelta {
                        role: None,
                        content: None,
                    },
                    finish_reason: response.and_then(|item| item.status),
                }],
                usage,
            }))
        }
        "response.failed" => {
            let message = dto
                .response
                .and_then(|item| item.error)
                .and_then(|error| error.message)
                .unwrap_or_else(|| "OpenAI Responses response failed".to_string());
            Err(LlmError::InvalidRequest(message))
        }
        "error" => {
            let message = dto
                .message
                .or_else(|| dto.error.and_then(|error| error.message))
                .unwrap_or_else(|| "OpenAI Responses stream returned an error".to_string());
            Err(LlmError::InvalidRequest(message))
        }
        _ => Ok(None),
    }
}

/// 提取输出文本
fn extract_output_text(dto: &ResponseDto) -> String {
    if let Some(output_text) = &dto.output_text
        && !output_text.is_empty()
    {
        return output_text.clone();
    }

    let mut content = String::new();
    for item in dto.output.as_deref().unwrap_or_default() {
        for part in item.content.as_deref().unwrap_or_default() {
            if part.content_type.as_deref() == Some("output_text") {
                content.push_str(part.text.as_deref().unwrap_or_default());
            }
        }
    }

    content
}

/// Responses API 非流式响应
#[derive(Deserialize)]
struct ResponseDto {
    id: Option<String>,
    model: Option<String>,
    status: Option<String>,
    #[serde(rename = "output_text")]
    output_text: Option<String>,
    output: Option<Vec<ResponseOutputItemDto>>,
    usage: Option<ResponseUsageDto>,
    error: Option<ResponseErrorDto>,
}

/// Responses API 输出项
#[derive(Deserialize)]
struct ResponseOutputItemDto {
    content: Option<Vec<ResponseOutputContentDto>>,
}

/// Responses API 输出内容
#[derive(Deserialize)]
struct ResponseOutputContentDto {
    #[serde(rename = "type")]
    content_type: Option<String>,
    text: Option<String>,
}

/// Responses API 流式事件
#[derive(Deserialize)]
struct ResponseStreamEventDto {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(rename = "response_id")]
    response_id: Option<String>,
    delta: Option<String>,
    response: Option<ResponseDto>,
    message: Option<String>,
    error: Option<ResponseErrorDto>,
}

/// Responses API 错误详情
#[derive(Deserialize)]
struct ResponseErrorDto {
    message: Option<String>,
}

/// Responses API 用量统计
#[derive(Clone, Deserialize)]
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
