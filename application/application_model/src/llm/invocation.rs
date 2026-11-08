use serde::{Deserialize, Serialize};

/// 扩展参数的子节点保留重复属性和数字原文，不转成去重后的动态对象。
pub type LlmExtraBody = indexmap::IndexMap<String, Box<serde_json::value::RawValue>>;

/// 应用层角色与源契约保持数值映射，协议层自行转换字符串角色。
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, common::data_helper::DataEnum,
)]
#[serde(into = "i32", try_from = "i32")]
#[repr(i32)]
pub enum AppLlmRole {
    System,
    User,
    Assistant,
    Tool,
}
impl From<AppLlmRole> for i32 {
    fn from(value: AppLlmRole) -> Self {
        value as i32
    }
}
impl TryFrom<i32> for AppLlmRole {
    type Error = String;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::System),
            1 => Ok(Self::User),
            2 => Ok(Self::Assistant),
            3 => Ok(Self::Tool),
            _ => Err("无效的 LLM 角色".into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, common::property_helper::PropertyModel)]
#[serde(rename_all = "camelCase")]
#[property(model_name = "LlmChatMessageDto")]
pub struct AppLlmMessage {
    #[property(name = "Role", enumeration)]
    pub role: AppLlmRole,
    #[property(name = "Content")]
    pub content: String,
}

/// 温度、输出上限等提供商参数统一通过 extra_body 传递。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLlmChatRequest {
    pub model: String,
    pub messages: Vec<AppLlmMessage>,
    pub user: Option<String>,
    pub extra_body: Option<LlmExtraBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLlmUsage {
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub total_tokens: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLlmChoice {
    pub index: i32,
    pub message: AppLlmMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLlmChatResponse {
    pub id: Option<String>,
    pub model: String,
    pub choices: Vec<AppLlmChoice>,
    pub usage: Option<AppLlmUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, common::property_helper::PropertyModel)]
#[serde(rename_all = "camelCase")]
#[property(model_name = "LlmChatStreamDeltaDto")]
pub struct AppLlmStreamDelta {
    #[property(name = "Role", enumeration)]
    pub role: Option<AppLlmRole>,
    #[property(name = "Content")]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLlmStreamChoice {
    pub index: i32,
    pub delta: AppLlmStreamDelta,
    pub finish_reason: Option<String>,
}

/// 保留全部候选、模型、响应标识和用量；不把多个候选拼接成一条文本。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppLlmStreamChunk {
    pub id: Option<String>,
    pub model: String,
    pub choices: Vec<AppLlmStreamChoice>,
    pub usage: Option<AppLlmUsage>,
}
