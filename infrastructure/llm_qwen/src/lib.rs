/// Qwen Provider 设置
mod settings;

/// Qwen Provider 客户端
mod client;

use std::sync::Arc;

use llm::{LlmClientFactory, LlmError};

pub use client::{QWEN_PROVIDER, QwenClient};
pub use settings::QwenSettings;

/// 注册 Qwen Provider 客户端
pub fn register(factory: &mut LlmClientFactory, settings: QwenSettings) -> Result<(), LlmError> {
    let client = Arc::new(QwenClient::new(settings)?);
    factory.register(QWEN_PROVIDER, client);
    Ok(())
}
