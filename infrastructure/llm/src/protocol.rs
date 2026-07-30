use std::sync::Arc;

use crate::{
    LlmClient, LlmError, OpenAiCompatibleClient, OpenAiCompatibleSettings,
    anthropic::AnthropicMessagesClient, responses::OpenAiResponsesClient,
};

/// LLM 协议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProtocolType {
    /// OpenAI Chat Completions 兼容协议
    Chat = 0,

    /// OpenAI Responses API 协议
    Responses = 1,

    /// Anthropic Messages API 协议
    Anthropic = 2,
}

impl TryFrom<i32> for LlmProtocolType {
    type Error = LlmError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Chat),
            1 => Ok(Self::Responses),
            2 => Ok(Self::Anthropic),
            other => Err(LlmError::UnsupportedProtocol(other)),
        }
    }
}

/// LLM 模型运行时配置
#[derive(Debug, Clone)]
pub struct LlmModelConfig {
    /// API 端点地址
    pub endpoint: String,

    /// 接口密钥
    pub api_key: String,

    /// 模型标识
    pub model_id: String,

    /// 协议类型
    pub protocol_type: i32,
}

impl LlmModelConfig {
    /// 解析协议类型
    pub fn protocol(&self) -> Result<LlmProtocolType, LlmError> {
        LlmProtocolType::try_from(self.protocol_type)
    }
}

/// 根据模型配置创建 LLM 客户端
pub fn create_client(
    http_client: reqwest::Client,
    config: LlmModelConfig,
) -> Result<Arc<dyn LlmClient>, LlmError> {
    validate_config(&config)?;
    let protocol = config.protocol()?;

    match protocol {
        LlmProtocolType::Chat => Ok(Arc::new(OpenAiCompatibleClient::new(
            http_client,
            OpenAiCompatibleSettings::new_endpoint(config.endpoint, config.api_key),
        ))),
        LlmProtocolType::Responses => Ok(Arc::new(OpenAiResponsesClient::new(http_client, config))),
        LlmProtocolType::Anthropic => {
            Ok(Arc::new(AnthropicMessagesClient::new(http_client, config)))
        }
    }
}

/// 校验 LLM 模型运行时配置
fn validate_config(config: &LlmModelConfig) -> Result<(), LlmError> {
    if config.endpoint.trim().is_empty() {
        return Err(LlmError::InvalidRequest(
            "LLM model endpoint is required".to_string(),
        ));
    }

    if config.api_key.trim().is_empty() {
        return Err(LlmError::InvalidRequest(
            "LLM model api key is required".to_string(),
        ));
    }

    if config.model_id.trim().is_empty() {
        return Err(LlmError::InvalidRequest(
            "LLM model id is required".to_string(),
        ));
    }

    Ok(())
}
