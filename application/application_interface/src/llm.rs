use async_trait::async_trait;
use common::app_error::AppError;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 应用层 LLM 消息角色
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppLlmRole {
    /// 系统消息
    System,

    /// 用户消息
    User,

    /// 助手消息
    Assistant,
}

/// 应用层 LLM 对话消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppLlmMessage {
    /// 消息角色
    pub role: AppLlmRole,

    /// 消息内容
    pub content: String,
}

/// 应用层 LLM 对话请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppLlmChatRequest {
    /// 模型名称
    pub model: String,

    /// 对话消息列表
    pub messages: Vec<AppLlmMessage>,

    /// 温度参数
    pub temperature: Option<f32>,

    /// 最大输出 Token 数
    pub max_tokens: Option<u32>,

    /// 额外请求参数
    pub extra_body: Option<Map<String, Value>>,
}

/// 应用层 LLM 用量信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppLlmUsage {
    /// 提示词 Token 数
    pub prompt_tokens: Option<u32>,

    /// 输出 Token 数
    pub completion_tokens: Option<u32>,

    /// 总 Token 数
    pub total_tokens: Option<u32>,
}

/// 应用层 LLM 对话响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppLlmChatResponse {
    /// 响应 ID
    pub response_id: Option<String>,

    /// 实际使用的模型名称
    pub model: String,

    /// 返回消息列表
    pub messages: Vec<AppLlmMessage>,

    /// 用量信息
    pub usage: Option<AppLlmUsage>,
}

/// 应用层 LLM 流式分片
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppLlmStreamChunk {
    /// 增量文本
    pub delta: Option<String>,

    /// 是否结束
    pub done: bool,
}

/// 应用层 LLM 客户端抽象
#[async_trait]
pub trait AppLlmClient: Send + Sync {
    /// 执行非流式对话
    async fn chat(&self, request: AppLlmChatRequest) -> Result<AppLlmChatResponse, AppError>;

    /// 执行流式对话
    fn chat_stream(
        &self,
        request: AppLlmChatRequest,
    ) -> BoxStream<'_, Result<AppLlmStreamChunk, AppError>>;
}

/// 应用层 LLM 客户端工厂抽象
pub trait AppLlmClientFactory: Send + Sync {
    /// 按 Provider 获取应用层 LLM 客户端
    ///
    /// 由宿主负责把 Provider 标识解析为可直接调用的应用层客户端
    fn require_client(&self, provider: &str) -> Result<Box<dyn AppLlmClient + '_>, AppError>;
}
