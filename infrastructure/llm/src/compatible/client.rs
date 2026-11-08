use crate::payload::Payload;
use async_trait::async_trait;
use futures_util::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use source_generator_core::ProtocolModel;

use super::settings::OpenAiCompatibleSettings;
use crate::{LLM_REQUEST_TIMEOUT, LlmClient, LlmError, models::*};

/// OpenAI Chat Completions 兼容协议客户端。
pub struct OpenAiCompatibleClient {
    http_client: reqwest::Client,
    settings: OpenAiCompatibleSettings,
    provider_name: String,
}

impl OpenAiCompatibleClient {
    pub fn new(http_client: reqwest::Client, settings: OpenAiCompatibleSettings) -> Self {
        Self {
            http_client,
            settings,
            provider_name: "LLM".into(),
        }
    }

    /// 动态模型工厂使用模型标识作为提供商诊断名称。
    pub fn with_provider_name(mut self, name: impl Into<String>) -> Self {
        self.provider_name = name.into();
        self
    }

    fn chat_url(&self) -> String {
        self.settings
            .endpoint
            .as_ref()
            .map(|endpoint| endpoint.trim().to_owned())
            .unwrap_or_else(|| {
                format!(
                    "{}/chat/completions",
                    self.settings.base_url.trim_end_matches('/')
                )
            })
    }

    fn validate(&self, request: &ChatRequest) -> Result<(), LlmError> {
        let endpoint = self
            .settings
            .endpoint
            .as_deref()
            .unwrap_or(&self.settings.base_url);
        if endpoint.trim().is_empty() || self.settings.api_key.trim().is_empty() {
            return Err(LlmError::InvalidRequest(
                "LLM endpoint and api key are required".into(),
            ));
        }
        if request.model.trim().is_empty() {
            return Err(LlmError::InvalidRequest("LLM model is required".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl LlmClient for OpenAiCompatibleClient {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        self.validate(&request)?;
        let body = build_payload(&request, false)?;
        let response = self
            .http_client
            .post(self.chat_url())
            .timeout(LLM_REQUEST_TIMEOUT)
            .bearer_auth(&self.settings.api_key)
            .json(&body)
            .header(reqwest::header::CONTENT_TYPE, crate::JSON_CONTENT_TYPE)
            .send()
            .await?;
        let wire: WireResponse = crate::http_response::read_json(
            response,
            &format!("{} response is empty.", self.provider_name),
        )
        .await?;
        Ok(ChatResponse {
            id: wire.id,
            model: wire.model.unwrap_or_default(),
            choices: wire
                .choices
                .unwrap_or_default()
                .into_iter()
                .map(|choice| {
                    let choice = crate::wire::require_object(choice)?;
                    let message = choice.message.unwrap_or_default();
                    Ok(ChatChoice {
                        index: choice.index,
                        message: ChatMessage {
                            role: parse_role(message.role.as_deref()),
                            content: message.content.unwrap_or_default(),
                        },
                        finish_reason: choice.finish_reason,
                    })
                })
                .collect::<Result<_, LlmError>>()?,
            usage: wire.usage.map(WireUsage::into_usage),
        })
    }

    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxStream<'_, Result<ChatStreamChunk, LlmError>> {
        Box::pin(async_stream::try_stream! {
            self.validate(&request)?;
            let body = build_payload(&request, true)?;
            let response = crate::streaming::send(self.http_client.post(self.chat_url())
                .bearer_auth(&self.settings.api_key).json(&body)
                .header(reqwest::header::CONTENT_TYPE, crate::JSON_CONTENT_TYPE)).await?;
            crate::http_response::ensure_success(&response)?;
            let mut lines = crate::streaming::lines(response);
            while let Some(line) = lines.next().await {
                let line = line?;
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data.is_empty() { continue; }
                if data.eq_ignore_ascii_case("[DONE]") { return; }
                let wire: Option<WireStreamChunk> = crate::wire::from_slice(data.as_bytes())
                    .map_err(|_| LlmError::StreamParse(format!("{} stream returned invalid JSON.", self.provider_name)))?;
                let wire = wire.ok_or_else(|| LlmError::StreamParse(format!("{} stream returned an empty JSON payload.", self.provider_name)))?;
                if let Some(error) = wire.error {
                    Err(LlmError::StreamParse(error_message(error, &self.provider_name)))?;
                }
                yield ChatStreamChunk {
                    id: wire.id, model: wire.model.unwrap_or_else(|| request.model.clone()),
                    choices: wire.choices.unwrap_or_default().into_iter().map(|choice| {
                        let choice = crate::wire::require_object(choice)?;
                        let delta = choice.delta.unwrap_or_default();
                        Ok(ChatStreamChoiceDelta { index: choice.index, delta: ChatStreamDelta {
                            role: delta.role.as_deref().filter(|role| !role.trim().is_empty())
                                .map(|role| parse_role(Some(role))), content: delta.content,
                        }, finish_reason: choice.finish_reason })
                    }).collect::<Result<_, LlmError>>()?,
                    usage: wire.usage.map(WireUsage::into_usage),
                };
            }
        })
    }
}

fn build_payload(request: &ChatRequest, stream: bool) -> Result<Payload, LlmError> {
    let mut payload = Payload::new();
    payload.insert("model".into(), json!(request.model));
    payload.insert(
        "messages".into(),
        Value::Array(
            request
                .messages
                .iter()
                .map(|message| json!({"role": message.role, "content": message.content}))
                .collect(),
        ),
    );
    payload.insert("stream".into(), json!(stream));
    if let Some(user) = request.user.as_ref().filter(|user| !user.trim().is_empty()) {
        payload.insert("user".into(), json!(user));
    }
    if let Some(extra) = &request.extra_body {
        payload.merge_extra(extra)?;
    }
    Ok(payload)
}

fn parse_role(role: Option<&str>) -> ChatRole {
    match role.unwrap_or_default().to_ascii_lowercase().as_str() {
        "system" => ChatRole::System,
        "user" => ChatRole::User,
        "tool" => ChatRole::Tool,
        _ => ChatRole::Assistant,
    }
}

fn error_message(error: crate::wire::ErrorValue, provider_name: &str) -> String {
    if let Some(message) = error.as_str() {
        return message.to_owned();
    }
    let message = error.string_property("message");
    let mut details = Vec::new();
    for key in ["code", "type"] {
        if let Some(value) = error
            .string_property(key)
            .filter(|value| !value.trim().is_empty())
            && !details.contains(&value)
        {
            details.push(value);
        }
    }
    match (message, details.is_empty()) {
        (Some(message), true) => message.to_owned(),
        (Some(message), false) if !message.trim().is_empty() => {
            format!("{message} ({})", details.join(", "))
        }
        (_, false) => details.join(", "),
        _ => format!("{provider_name} stream returned an error."),
    }
}

#[derive(Default, ProtocolModel)]
struct WireMessage {
    role: Option<String>,
    content: Option<String>,
}
#[derive(ProtocolModel)]
struct WireUsage {
    prompt_tokens: Option<i32>,
    completion_tokens: Option<i32>,
    total_tokens: Option<i32>,
}
impl WireUsage {
    fn into_usage(self) -> Usage {
        Usage {
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
        }
    }
}
#[derive(ProtocolModel)]
struct WireChoice {
    #[serde(default)]
    index: i32,
    message: Option<WireMessage>,
    finish_reason: Option<String>,
}
#[derive(ProtocolModel)]
struct WireResponse {
    id: Option<String>,
    model: Option<String>,
    choices: Option<Vec<Option<WireChoice>>>,
    usage: Option<WireUsage>,
}
#[derive(ProtocolModel)]
struct WireStreamChoice {
    #[serde(default)]
    index: i32,
    delta: Option<WireMessage>,
    finish_reason: Option<String>,
}
#[derive(ProtocolModel)]
struct WireStreamChunk {
    id: Option<String>,
    model: Option<String>,
    choices: Option<Vec<Option<WireStreamChoice>>>,
    usage: Option<WireUsage>,
    error: Option<crate::wire::ErrorValue>,
}
