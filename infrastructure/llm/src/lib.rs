/// LLM 操作错误类型
pub mod error;

/// LLM 数据模型（消息、请求、响应、流式分片等）
pub mod models;

/// LLM 客户端统一 trait
pub mod client;

/// LLM 客户端工厂
pub mod factory;

/// OpenAI 兼容协议实现
pub mod compatible;

pub use client::LlmClient;
pub use compatible::{OpenAiCompatibleClient, OpenAiCompatibleSettings};
/// 常用类型统一导出，方便调用方不必深入子模块
pub use error::LlmError;
pub use factory::LlmClientFactory;
pub use models::{
    ChatChoice, ChatMessage, ChatRequest, ChatResponse, ChatRole, ChatStreamChoiceDelta,
    ChatStreamChunk, ChatStreamDelta, Usage,
};
