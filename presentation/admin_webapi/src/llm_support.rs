#![allow(clippy::type_complexity)]

use application_interface::{
    context::{AppContext, LlmContext},
    email::AppEmailSender,
    error::AppError,
    llm::{AppLlmClient, AppLlmClientFactory},
    storage::AppFileStorage,
};
use application_service_llm::llm::{
    app_service::LlmAppService, client_adapter::FactoryAppLlmClient,
    conversation_manage_service::LlmConversationManageService,
};
use distributed_cache::DistributedCache;
use distributed_cache_memory::InMemoryCache;
use distributed_cache_redis::RedisCache;
use distributed_lock::DistributedLock;
use distributed_lock_memory::InMemoryLock;
use distributed_lock_redis::RedisLock;
use email_smtp::{SmtpEmailSender, SmtpEmailSettings};
use file_storage_ali_cloud::{AliyunOssFileStorage, AliyunOssFileStorageSettings};
use file_storage_tencent_cloud::{TencentCosFileStorage, TencentCosFileStorageSettings};
use llm::{LlmClientFactory, OpenAiCompatibleClient, OpenAiCompatibleSettings};
use logger::{LoggerComponent, LoggerRuntime};
use logger_database::init_database_logger;
use logger_local_file::init_local_file_logger;
use std::sync::Arc;
use std::{future::Future, pin::Pin};
use webapi_core::{
    host::WebApiLoggerBuilderContext,
    settings::{CacheComponent, LockComponent, Settings},
};

use crate::state::AppState;

/// 构建管理端 LLM 客户端工厂
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

/// 构建管理端分布式锁
pub fn build_distributed_lock(
    settings: &Settings,
) -> Pin<Box<dyn Future<Output = Result<Arc<dyn DistributedLock>, String>> + Send + '_>> {
    Box::pin(async move {
        match settings.lock.component {
            LockComponent::InMemory => {
                Ok(Arc::new(InMemoryLock::new()) as Arc<dyn DistributedLock>)
            }
            LockComponent::Redis => Ok(Arc::new(
                RedisLock::new(&settings.lock.connection, &settings.lock.prefix)
                    .await
                    .map_err(|error| format!("Redis 分布式锁初始化失败: {error}"))?,
            ) as Arc<dyn DistributedLock>),
        }
    })
}

/// 构建管理端缓存
pub fn build_cache(
    settings: &Settings,
) -> Pin<Box<dyn Future<Output = Result<Arc<dyn DistributedCache>, String>> + Send + '_>> {
    Box::pin(async move {
        match settings.cache.component {
            CacheComponent::InMemory => {
                Ok(Arc::new(InMemoryCache::new()) as Arc<dyn DistributedCache>)
            }
            CacheComponent::Redis => Ok(Arc::new(
                RedisCache::new(&settings.cache.connection, &settings.cache.prefix)
                    .await
                    .map_err(|error| format!("Redis 缓存初始化失败: {error}"))?,
            ) as Arc<dyn DistributedCache>),
        }
    })
}

/// 构建管理端日志运行时
pub fn build_logger_runtime(
    settings: &Settings,
    context: &WebApiLoggerBuilderContext,
) -> Result<LoggerRuntime, String> {
    match settings.logger.component {
        LoggerComponent::Database => init_database_logger(
            &settings.logger,
            Arc::clone(&context.db_pool),
            context.identifier_generator.clone(),
        ),
        LoggerComponent::LocalFile => init_local_file_logger(&settings.logger),
    }
    .map_err(|error| format!("日志组件初始化失败: {error}"))
}

/// 构建管理端文件存储
pub fn build_file_storage(settings: &Settings) -> Result<Option<Arc<dyn AppFileStorage>>, String> {
    match settings.file.component {
        webapi_core::settings::FileStorageComponent::Local => Ok(None),
        webapi_core::settings::FileStorageComponent::AliyunOss => Ok(Some(Arc::new(
            AliyunOssFileStorage::new(AliyunOssFileStorageSettings {
                region: settings.file.aliyun_oss.region.clone(),
                use_internal_endpoint: settings.file.aliyun_oss.use_internal_endpoint,
                access_key_id: settings.file.aliyun_oss.access_key_id.clone(),
                access_key_secret: settings.file.aliyun_oss.access_key_secret.clone(),
                bucket_name: settings.file.aliyun_oss.bucket_name.clone(),
                url: settings.file.public_base_url.clone(),
            })
            .map_err(|error| error.message().to_string())?,
        ))),
        webapi_core::settings::FileStorageComponent::TencentCos => Ok(Some(Arc::new(
            TencentCosFileStorage::new(TencentCosFileStorageSettings {
                app_id: settings.file.tencent_cos.app_id.clone(),
                region: settings.file.tencent_cos.region.clone(),
                secret_id: settings.file.tencent_cos.secret_id.clone(),
                secret_key: settings.file.tencent_cos.secret_key.clone(),
                bucket_name: settings.file.tencent_cos.bucket_name.clone(),
                url: settings.file.public_base_url.clone(),
            })
            .map_err(|error| error.message().to_string())?,
        ))),
    }
}

/// 构建管理端邮件发送器
pub fn build_email_sender(settings: &Settings) -> Result<Arc<dyn AppEmailSender>, String> {
    Ok(Arc::new(SmtpEmailSender::new(SmtpEmailSettings {
        host: settings.message.smtp.host.clone(),
        port: settings.message.smtp.port,
        username: settings.message.smtp.username.clone(),
        password: settings.message.smtp.password.clone(),
    })))
}

/// 管理端 LLM 应用服务适配器
pub struct AdminLlmApplicationAdapter<'a> {
    /// WebAPI 宿主状态
    state: &'a AppState,
}

impl<'a> AdminLlmApplicationAdapter<'a> {
    /// 创建管理端 LLM 应用服务适配器
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

    /// 创建 LLM 应用配置服务
    pub fn llm_app(&self) -> LlmAppService<'_> {
        LlmAppService::new(self.llm_context())
    }

    /// 创建 LLM 对话记录管理服务
    pub fn llm_conversation_manage(&self) -> LlmConversationManageService<'_> {
        LlmConversationManageService::new(self.llm_context())
    }
}

impl AppLlmClientFactory for AdminLlmApplicationAdapter<'_> {
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
