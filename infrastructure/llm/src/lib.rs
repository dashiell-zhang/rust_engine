/// LLM 操作错误类型
pub mod error;

/// LLM 单次请求整体超时时间
pub(crate) const LLM_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// LLM 数据模型（消息、请求、响应、流式分片等）
pub mod models;

/// LLM 客户端统一 trait
pub mod client;

/// OpenAI 兼容协议实现
pub mod compatible;

/// OpenAI Responses API 协议实现
pub mod responses;

/// Anthropic Messages API 协议实现
pub mod anthropic;

/// LLM 协议配置与动态客户端创建
pub mod protocol;

pub use client::LlmClient;
pub use compatible::{OpenAiCompatibleClient, OpenAiCompatibleSettings};
/// 常用类型统一导出，方便调用方不必深入子模块
pub use error::LlmError;
pub use models::{
    ChatChoice, ChatMessage, ChatRequest, ChatResponse, ChatRole, ChatStreamChoiceDelta,
    ChatStreamChunk, ChatStreamDelta, Usage,
};
pub use protocol::{LlmModelConfig, LlmProtocolType, create_client};
