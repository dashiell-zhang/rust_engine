#[cfg(feature = "debug-demo")]
use application_interface::{
    context::{AppContext, LlmContext},
    error::AppError,
    llm::{AppLlmClient, AppLlmClientFactory},
};
#[cfg(feature = "debug-demo")]
use application_service_llm::llm::{
    client_adapter::FactoryAppLlmClient, invoke_service::LlmInvokeService,
};
use llm::LlmClientFactory;
#[cfg(feature = "debug-demo")]
use llm::{OpenAiCompatibleClient, OpenAiCompatibleSettings};
#[cfg(not(feature = "debug-demo"))]
use std::collections::BTreeSet;
#[cfg(feature = "debug-demo")]
use std::sync::Arc;
use webapi_core::settings::Settings;

#[cfg(feature = "debug-demo")]
use crate::state::AppState;

/// 构建客户端 LLM 客户端工厂
#[cfg(feature = "debug-demo")]
pub fn build_llm_client_factory(settings: &Settings) -> Result<LlmClientFactory, String> {
    let mut factory = LlmClientFactory::new();

    for (provider_key, provider_settings) in &settings.llm.providers {
        let provider = provider_key.trim();
        if provider.is_empty() {
            continue;
        }

        let client = OpenAiCompatibleClient::new(OpenAiCompatibleSettings::new(
            &provider_settings.base_url,
            &provider_settings.api_key,
        ))
        .map_err(|error| {
            format!("LLM Provider 初始化失败: provider={provider_key}, error={error}")
        })?;

        factory.register(provider, Arc::new(client));
    }

    Ok(factory)
}

/// 构建客户端 LLM 客户端工厂
#[cfg(not(feature = "debug-demo"))]
pub fn build_llm_client_factory(settings: &Settings) -> Result<LlmClientFactory, String> {
    let providers = settings
        .llm
        .providers
        .keys()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>();

    if !providers.is_empty() {
        tracing::warn!(
            target: "client_webapi.startup",
            providers = ?providers,
            "当前构建未启用 debug-demo，已跳过客户端 Demo LLM Provider 装配"
        );
    }

    Ok(LlmClientFactory::new())
}

/// 客户端 LLM 应用服务适配器
#[cfg(feature = "debug-demo")]
pub struct ClientLlmApplicationAdapter<'a> {
    /// WebAPI 宿主状态
    state: &'a AppState,
}

#[cfg(feature = "debug-demo")]
impl<'a> ClientLlmApplicationAdapter<'a> {
    /// 创建客户端 LLM 应用服务适配器
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// 获取 LLM 模块通用上下文
    fn llm_context(&self) -> LlmContext<'_> {
        LlmContext {
            app: self.app_context(),
            llm_client_factory: self,
        }
    }

    /// 获取应用层通用上下文
    fn app_context(&self) -> AppContext<'_> {
        AppContext {
            db: self.state.db_pool.as_ref(),
            id_generator: &self.state.id_generator,
            current_user: None,
        }
    }

    /// 创建 LLM 运行时调用服务
    pub fn llm_invoke(&self) -> LlmInvokeService<'_> {
        LlmInvokeService::new(self.llm_context())
    }
}

#[cfg(feature = "debug-demo")]
impl AppLlmClientFactory for ClientLlmApplicationAdapter<'_> {
    /// 按 Provider 获取应用层 LLM 客户端
    fn require_client(&self, provider: &str) -> Result<Box<dyn AppLlmClient + '_>, AppError> {
        self.state
            .llm_client_factory
            .get_client(provider)
            .ok_or_else(|| {
                AppError::internal(format!("获取 LLM 客户端失败: 未注册 provider {provider}"))
            })?;

        Ok(Box::new(FactoryAppLlmClient::new(
            &self.state.llm_client_factory,
            provider.to_string(),
        )))
    }
}
