#![allow(clippy::type_complexity)]
use webapi_support::WebApiApplicationExt;

use application_interface::{email::AppEmailSender, storage::AppFileStorage};
use distributed_cache::DistributedCache;
use distributed_cache_memory::InMemoryCache;
use distributed_cache_redis::RedisCache;
use distributed_lock::DistributedLock;
use distributed_lock_memory::InMemoryLock;
use distributed_lock_redis::RedisLock;
use email_smtp::{SmtpEmailSender, SmtpEmailSettings};
use file_storage_ali_cloud::{AliyunOssFileStorage, AliyunOssFileStorageSettings};
use file_storage_tencent_cloud::{TencentCosFileStorage, TencentCosFileStorageSettings};
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
pub fn build_logger_runtime<'a>(
    settings: &'a Settings,
    context: &'a WebApiLoggerBuilderContext<'a>,
) -> Pin<Box<dyn Future<Output = Result<LoggerRuntime, String>> + Send + 'a>> {
    Box::pin(async move {
        match settings.logger.component {
            LoggerComponent::Database => {
                // 分区维护必须在所有宿主使用同一连接和前缀，不能沿用宿主专属业务锁前缀。
                let maintenance_lock: Arc<dyn DistributedLock> = match settings.lock.component {
                    LockComponent::InMemory => Arc::new(InMemoryLock::new()),
                    LockComponent::Redis => Arc::new(
                        RedisLock::new(
                            if settings
                                .logger
                                .maintenance_lock_connection
                                .trim()
                                .is_empty()
                            {
                                &settings.lock.connection
                            } else {
                                &settings.logger.maintenance_lock_connection
                            },
                            &settings.logger.maintenance_lock_prefix,
                        )
                        .await
                        .map_err(|error| format!("日志维护锁初始化失败: {error}"))?,
                    ),
                };
                init_database_logger(
                    &settings.logger,
                    context
                        .resources
                        .get::<webapi_support::resources::BusinessResources>()?
                        .log_db_pool
                        .clone()
                        .ok_or("数据库日志写连接未装配")?,
                    context.identifier_generator.clone(),
                    settings
                        .logger
                        .write_connection(&settings.database.connection),
                    maintenance_lock,
                )
                .await
            }
            LoggerComponent::LocalFile => {
                init_local_file_logger(&settings.logger).map_err(|error| error.to_string())
            }
        }
        .map_err(|error| format!("日志组件初始化失败: {error}"))
    })
}

/// 构建管理端文件存储
pub fn build_file_storage(
    settings: &Settings,
    http_clients: Arc<application_runtime::http_client::HttpClientFactory>,
) -> Result<Option<Arc<dyn AppFileStorage>>, String> {
    match settings.file.component {
        webapi_core::settings::FileStorageComponent::Local => Ok(None),
        webapi_core::settings::FileStorageComponent::AliyunOss => Ok(Some(Arc::new(
            AliyunOssFileStorage::new(
                AliyunOssFileStorageSettings {
                    region: settings.file.aliyun_oss.region.clone(),
                    use_internal_endpoint: settings.file.aliyun_oss.use_internal_endpoint,
                    access_key_id: settings.file.aliyun_oss.access_key_id.clone(),
                    access_key_secret: settings.file.aliyun_oss.access_key_secret.clone(),
                    bucket_name: settings.file.aliyun_oss.bucket_name.clone(),
                    url: settings.file.public_base_url.clone(),
                },
                http_clients,
            )
            .map_err(|error| error.message().to_string())?,
        ))),
        webapi_core::settings::FileStorageComponent::TencentCos => Ok(Some(Arc::new(
            TencentCosFileStorage::new(
                TencentCosFileStorageSettings {
                    app_id: settings.file.tencent_cos.app_id.clone(),
                    region: settings.file.tencent_cos.region.clone(),
                    secret_id: settings.file.tencent_cos.secret_id.clone(),
                    secret_key: settings.file.tencent_cos.secret_key.clone(),
                    bucket_name: settings.file.tencent_cos.bucket_name.clone(),
                    url: settings.file.public_base_url.clone(),
                },
                http_clients,
            )
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

    /// 创建 LLM 应用配置服务
    pub fn llm_app(&self) -> Box<dyn application_interface::services::llm_app::LlmAppService + '_> {
        self.state.services().llm_app()
    }

    /// 创建 LLM 模型配置服务
    pub fn llm_model(
        &self,
    ) -> Box<dyn application_interface::services::llm_model::LlmModelService + '_> {
        self.state.services().llm_model()
    }

    /// 创建 LLM 对话记录管理服务
    pub fn llm_conversation_manage(
        &self,
    ) -> Box<dyn application_interface::services::llm_conversation::LlmConversationManageService + '_>
    {
        self.state.services().llm_conversation_manage()
    }
}

/// 明确选择事件目标；后端连接由事件组件独立持有。
pub fn build_event_transport(
    settings: &Settings,
    identifiers: &common::identifier_generator::IdentifierGenerator,
) -> Result<Arc<dyn event_bus::EventTransport>, String> {
    settings.event_bus.validate()?;
    let explicit = settings.event_bus.connection.trim();
    match settings.event_bus.provider {
        event_bus::EventProvider::Redis => {
            let connection = if explicit.is_empty() {
                &settings.cache.connection
            } else {
                explicit
            };
            if connection.trim().is_empty() {
                return Err("事件 Redis 连接未配置".to_string());
            }
            Ok(Arc::new(event_bus_redis::RedisEventTransport::new(
                connection,
                &settings.event_bus.scope,
            )?))
        }
        event_bus::EventProvider::Database => {
            let connection = if explicit.is_empty() {
                &settings.database.connection
            } else {
                explicit
            };
            let options = webapi_core::startup::WebApiStartup::build_postgres_connect_options(
                connection,
                &settings.host.database_pool,
            );
            Ok(Arc::new(event_bus_database::DatabaseEventTransport::new(
                options,
                &settings.event_bus.scope,
                identifiers.clone(),
            )))
        }
    }
}
