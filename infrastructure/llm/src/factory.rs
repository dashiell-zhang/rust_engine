use std::collections::HashMap;
use std::sync::Arc;

use crate::client::LlmClient;
use crate::error::LlmError;

/// LLM 客户端工厂
///
/// 管理按 Provider 标识注册的 LLM 客户端
/// 适用于宿主启动时预注册已知 Provider
pub struct LlmClientFactory {
    /// 已注册的 LLM 客户端映射表
    clients: HashMap<String, Arc<dyn LlmClient>>,
}

impl LlmClientFactory {
    /// 创建空的 LLM 客户端工厂
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    /// 注册 LLM 客户端
    ///
    /// 若 Provider 已存在则覆盖
    pub fn register(
        &mut self,
        provider: impl Into<String>,
        client: Arc<dyn LlmClient>,
    ) -> &mut Self {
        self.clients
            .insert(normalize_provider(&provider.into()), client);
        self
    }

    /// 获取指定 Provider 的客户端
    ///
    /// 若 Provider 未注册则返回 `None`
    pub fn get_client(&self, provider: &str) -> Option<Arc<dyn LlmClient>> {
        self.clients.get(&normalize_provider(provider)).cloned()
    }

    /// 获取指定 Provider 的客户端，未注册时返回错误
    pub fn require_client(&self, provider: &str) -> Result<Arc<dyn LlmClient>, LlmError> {
        self.get_client(provider)
            .ok_or_else(|| LlmError::ProviderNotFound(provider.to_string()))
    }

    /// 返回所有已注册的 Provider 标识列表
    pub fn providers(&self) -> Vec<&str> {
        self.clients.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for LlmClientFactory {
    fn default() -> Self {
        Self::new()
    }
}

/// 归一化 Provider 标识
fn normalize_provider(provider: &str) -> String {
    provider.trim().to_ascii_lowercase()
}
