/// OpenAI 兼容客户端设置
mod settings;

/// OpenAI 兼容客户端实现
mod client;

pub use client::OpenAiCompatibleClient;
pub use settings::OpenAiCompatibleSettings;
