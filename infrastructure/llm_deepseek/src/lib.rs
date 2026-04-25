/// DeepSeek Provider 设置
mod settings;

/// DeepSeek Provider 客户端
mod client;

use std::sync::Arc;

use llm::{LlmClientFactory, LlmError};

pub use client::{DEEPSEEK_PROVIDER, DeepSeekClient};
pub use settings::DeepSeekSettings;

/// 注册 DeepSeek Provider 客户端
pub fn register(
    factory: &mut LlmClientFactory,
    settings: DeepSeekSettings,
) -> Result<(), LlmError> {
    let client = Arc::new(DeepSeekClient::new(settings)?);
    factory.register(DEEPSEEK_PROVIDER, client);
    Ok(())
}
