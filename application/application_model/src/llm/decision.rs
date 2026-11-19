//! 决策应用的结构化结果。
use indexmap::IndexMap;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmDecisionResponseDto {
    pub model: String,
    pub answers: IndexMap<String, LlmDecisionAnswerDto>,
    pub request_id: Option<String>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub latency_ms: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmDecisionAnswerDto {
    #[serde(rename = "type")]
    pub kind: String,
    pub choice: Option<String>,
    pub yes_probability: Option<f64>,
    pub score: Option<f64>,
    pub confidence: Option<f64>,
    pub probabilities: Option<IndexMap<String, f64>>,
    pub legend: Option<IndexMap<String, String>>,
}
