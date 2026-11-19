//! System One 非流式结构化决策协议。
use crate::{LlmError, LlmModelConfig};
use async_trait::async_trait;
use common::json_helper::input::OrderedJson;
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

use source_generator_core::ProtocolModel;

// Web JSON 允许供应商以字符串表示数字。仅在已声明的数字字段上转换。
struct WebNumber<T>(T);
impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for WebNumber<T> {
    fn deserialize<D: serde::Deserializer<'de>>(reader: D) -> Result<Self, D::Error> {
        let raw = Box::<serde_json::value::RawValue>::deserialize(reader)?;
        let value = raw.get();
        let number = if value.starts_with('"') {
            let text: String = serde_json::from_str(value).map_err(serde::de::Error::custom)?;
            if text.trim() != text {
                return Err(serde::de::Error::custom("决策数值格式无效"));
            }
            serde_json::from_str(&text)
        } else {
            serde_json::from_str(value)
        };
        number.map(Self).map_err(serde::de::Error::custom)
    }
}

#[derive(ProtocolModel)]
struct WireAnswer {
    #[serde(rename = "type")]
    kind: String,
    choice: Option<String>,
    noul: Option<WebNumber<f64>>,
    score: Option<WebNumber<f64>>,
    confidence: Option<WebNumber<f64>>,
    probabilities: Option<IndexMap<String, WebNumber<f64>>>,
    legend: Option<IndexMap<String, String>>,
}
pub struct Answer {
    pub kind: String,
    pub choice: Option<String>,
    pub noul: Option<f64>,
    pub score: Option<f64>,
    pub confidence: Option<f64>,
    pub probabilities: Option<IndexMap<String, f64>>,
    pub legend: Option<IndexMap<String, String>>,
}
impl From<WireAnswer> for Answer {
    fn from(a: WireAnswer) -> Self {
        Self {
            kind: a.kind,
            choice: a.choice,
            noul: a.noul.map(|v| v.0),
            score: a.score.map(|v| v.0),
            confidence: a.confidence.map(|v| v.0),
            probabilities: a
                .probabilities
                .map(|map| map.into_iter().map(|(k, v)| (k, v.0)).collect()),
            legend: a.legend,
        }
    }
}
#[derive(ProtocolModel)]
struct WireUsage {
    input_tokens: Option<WebNumber<i32>>,
    output_tokens: Option<WebNumber<i32>>,
}
pub struct Usage {
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
}
#[derive(ProtocolModel)]
struct WireResponse {
    model: String,
    answers: IndexMap<String, WireAnswer>,
    request_id: Option<String>,
    usage: Option<WireUsage>,
    latency_ms: Option<WebNumber<f64>>,
}
pub struct DecisionResponse {
    pub model: String,
    pub answers: IndexMap<String, Answer>,
    pub request_id: Option<String>,
    pub usage: Option<Usage>,
    pub latency_ms: Option<f64>,
    pub request_json: String,
    pub raw_content: String,
}

#[derive(serde::Serialize)]
struct Question {
    #[serde(rename = "type")]
    kind: String,
    instructions: OrderedJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    criteria: Option<OrderedJson>,
}
pub struct DecisionRequest {
    state: OrderedJson,
    questions: IndexMap<String, Question>,
    extra: IndexMap<String, OrderedJson>,
}

fn description(v: &OrderedJson) -> bool {
    v.as_str().is_some_and(|s| !s.trim().is_empty())
}
fn content(v: &OrderedJson) -> bool {
    matches!(v, OrderedJson::Array(_) | OrderedJson::Object(_)) || description(v)
}

impl Question {
    fn parse(id: &str, node: OrderedJson) -> Result<Self, LlmError> {
        let error = || LlmError::InvalidRequest(format!("问题 {id} 的类型、选项或说明无效"));
        let OrderedJson::Object(fields) = node else {
            return Err(error());
        };
        let mut normalized = IndexMap::new();
        for (key, value) in fields {
            let key = key.to_ascii_lowercase();
            if !matches!(key.as_str(), "type" | "instructions" | "criteria")
                || normalized.insert(key, value).is_some()
            {
                return Err(error());
            }
        }
        let Some(OrderedJson::String(kind)) = normalized.shift_remove("type") else {
            return Err(error());
        };
        let instructions = normalized
            .shift_remove("instructions")
            .filter(content)
            .ok_or_else(error)?;
        let criteria = normalized
            .shift_remove("criteria")
            .filter(|v| !matches!(v, OrderedJson::Null));
        let valid = match (kind.as_str(), criteria.as_ref()) {
            ("choice", Some(OrderedJson::Object(options))) => {
                let mut seen = HashSet::new();
                !options.is_empty()
                    && options.iter().all(|(k, v)| {
                        !k.trim().is_empty()
                            && description(v)
                            && seen.insert(common::string_helper::ordinal_ignore_case_key(k))
                    })
            }
            ("noul", None) => true,
            ("noul", Some(OrderedJson::Object(options))) => {
                !options.is_empty()
                    && options
                        .iter()
                        .all(|(k, v)| matches!(k.as_str(), "true" | "false") && description(v))
            }
            ("score", Some(OrderedJson::Array(levels))) => {
                !levels.is_empty() && levels.iter().all(description)
            }
            _ => false,
        };
        if !valid || id.trim().is_empty() {
            return Err(error());
        }
        Ok(Self {
            kind,
            instructions,
            criteria,
        })
    }
}
impl DecisionRequest {
    pub fn from_template(mut payload: IndexMap<String, OrderedJson>) -> Result<Self, LlmError> {
        payload.shift_remove("model");
        let state = payload
            .shift_remove("state")
            .filter(content)
            .ok_or_else(|| LlmError::InvalidRequest("state 必须是对象、数组或非空文本".into()))?;
        let Some(OrderedJson::Object(nodes)) = payload.shift_remove("questions") else {
            return Err(LlmError::InvalidRequest("questions 必须是问题对象".into()));
        };
        if nodes.is_empty() {
            return Err(LlmError::InvalidRequest("请至少配置一个决策问题".into()));
        }
        let mut questions = IndexMap::new();
        for (id, node) in nodes {
            let question = Question::parse(&id, node)?;
            questions.insert(id, question);
        }
        if payload.keys().any(|k| {
            ["model", "state", "questions", "stream"]
                .iter()
                .any(|r| k.eq_ignore_ascii_case(r))
        }) {
            return Err(LlmError::InvalidRequest(
                "决策扩展字段不能覆盖协议字段或启用 stream".into(),
            ));
        }
        Ok(Self {
            state,
            questions,
            extra: payload,
        })
    }
    /// 注入的客户端可使用同一请求格式，模型标识来自运行配置。
    pub fn request_json(&self, model: &str) -> Result<String, LlmError> {
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            model: &'a str,
            state: &'a OrderedJson,
            questions: &'a IndexMap<String, Question>,
            #[serde(flatten)]
            extra: &'a IndexMap<String, OrderedJson>,
        }
        Ok(serde_json::to_string(&Payload {
            model,
            state: &self.state,
            questions: &self.questions,
            extra: &self.extra,
        })?)
    }

    pub fn validate_response(&self, resp: &DecisionResponse) -> Result<(), LlmError> {
        let questions = &self.questions;
        if resp.model.trim().is_empty() || questions.len() != resp.answers.len() {
            return Err(LlmError::InvalidResponse("决策响应缺少模型或答案".into()));
        }
        let probability = |v: f64| (0.0..=1.0).contains(&v);
        for (id, q) in questions {
            let error = || LlmError::InvalidResponse(format!("问题 {id} 的决策答案无效"));
            let a = resp
                .answers
                .get(id)
                .filter(|a| q.kind == a.kind)
                .ok_or_else(error)?;
            if a.kind == "noul" {
                if !a.noul.is_some_and(probability) {
                    return Err(error());
                }
                continue;
            }
            let keys: Vec<String> = match q.criteria.as_ref() {
                Some(OrderedJson::Object(options)) => options.keys().cloned().collect(),
                Some(OrderedJson::Array(levels)) => {
                    (0..levels.len()).map(|i| i.to_string()).collect()
                }
                _ => return Err(error()),
            };
            let valid_probs = a.probabilities.as_ref().is_some_and(|p| {
                p.len() == keys.len()
                    && keys
                        .iter()
                        .all(|k| p.get(k).is_some_and(|v| probability(*v)))
            });
            if !valid_probs || !a.confidence.is_some_and(probability) {
                return Err(error());
            }
            if a.kind == "choice" {
                if !a.choice.as_ref().is_some_and(|v| keys.contains(v)) {
                    return Err(error());
                }
            } else if !a
                .score
                .is_some_and(|v| (0.0..=(keys.len() - 1) as f64).contains(&v))
                || !a.legend.as_ref().is_some_and(|l| {
                    l.len() == keys.len() && keys.iter().all(|k| l.contains_key(k))
                })
            {
                return Err(error());
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait DecisionClient: Send + Sync {
    async fn decide(&self, request: DecisionRequest) -> Result<DecisionResponse, LlmError>;
}
pub(crate) struct SystemOneClient {
    http: reqwest::Client,
    config: LlmModelConfig,
}
impl SystemOneClient {
    pub(crate) fn new(http: reqwest::Client, config: LlmModelConfig) -> Self {
        Self { http, config }
    }
}
#[async_trait]
impl DecisionClient for SystemOneClient {
    async fn decide(&self, request: DecisionRequest) -> Result<DecisionResponse, LlmError> {
        let body = request.request_json(&self.config.model_id)?;
        let response = self
            .http
            .post(&self.config.endpoint)
            .timeout(crate::LLM_REQUEST_TIMEOUT)
            .bearer_auth(&self.config.api_key)
            .header("Content-Type", "application/json; charset=utf-8")
            .body(body.clone())
            .send()
            .await?;
        let status = response.status();
        let request_id = ["x-request-id", "x-acs-request-id"]
            .iter()
            .find_map(|k| response.headers().get(*k)?.to_str().ok().map(str::to_owned));
        let raw = response.text().await?;
        if !status.is_success() {
            return Err(provider_error(
                status.as_u16(),
                &raw,
                request_id,
                &self.config.api_key,
            ));
        }
        let wire: WireResponse = serde_json::from_str(&raw)?;
        let result = DecisionResponse {
            model: wire.model,
            answers: wire
                .answers
                .into_iter()
                .map(|(id, a)| (id, a.into()))
                .collect(),
            request_id: wire.request_id,
            latency_ms: wire.latency_ms.map(|v| v.0),
            usage: wire.usage.map(|u| Usage {
                input_tokens: u.input_tokens.map(|v| v.0),
                output_tokens: u.output_tokens.map(|v| v.0),
            }),
            raw_content: raw,
            request_json: body,
        };
        request.validate_response(&result)?;
        Ok(result)
    }
}

fn provider_error(status: u16, raw: &str, request_id: Option<String>, key: &str) -> LlmError {
    let root: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    let field = |v: &Value| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    let error = &root["error"];
    let code = field(&error["code"]).or_else(|| field(&root["code"]));
    let detail = [
        &error["message"],
        &root["message"],
        &root["detail"],
        error,
        &root["detail"][0]["msg"],
    ]
    .into_iter()
    .find_map(field);
    let id = field(&root["request_id"])
        .or_else(|| field(&error["request_id"]))
        .or(request_id);
    let mut parts = vec![format!("决策模型调用失败（HTTP {status}）")];
    for (prefix, v) in [("错误码 ", code), ("", detail), ("请求 ID ", id)] {
        if let Some(v) = v.filter(|s| !s.trim().is_empty()) {
            let redacted = format!("{prefix}{v}").replace(key, "[已隐藏]");
            let mut chars = redacted.chars();
            let mut part: String = chars.by_ref().take(512).collect();
            if chars.next().is_some() {
                part.push('…');
            }
            parts.push(part);
        }
    }
    LlmError::ApiError {
        status,
        message: parts.join("，"),
    }
}
