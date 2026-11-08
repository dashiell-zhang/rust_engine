#![allow(clippy::type_complexity)]

use std::{future::Future, pin::Pin, sync::Arc};

use crate::configuration::{SmsComponent, TaskComponentSettings};
use distributed_cache::DistributedCache;
use distributed_cache_memory::InMemoryCache;
use distributed_cache_redis::RedisCache;
use distributed_lock::DistributedLock;
use distributed_lock_memory::InMemoryLock;
use distributed_lock_redis::RedisLock;
use email::AppEmailSender;
use email_smtp::SmtpEmailSender;
use file_storage::AppFileStorage;
use logger::{LoggerComponent, LoggerRuntime};
use logger_database::init_database_logger;
use logger_local_file::init_local_file_logger;
use sms::AppSmsSender;
use sms_ali_cloud::AliyunSmsSender;
use sms_tencent_cloud::TencentSmsSender;
use task_service_core::{
    TaskServiceLoggerBuilderContext,
    settings::{CacheComponent, LockComponent, Settings},
};

/// 构建任务宿主分布式锁
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

/// 构建任务宿主文件存储
pub fn build_file_storage(
    settings: &crate::configuration::TaskComponentSettings,
    http_clients: Arc<http_client::HttpClientFactory>,
) -> Result<Option<Arc<dyn AppFileStorage>>, String> {
    settings
        .file_storage
        .build(&settings.file.public_base_url, http_clients)
}

/// 构建任务宿主分布式缓存
pub fn build_distributed_cache(
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

/// 构建任务宿主日志运行时
pub fn build_logger_runtime<'a>(
    settings: &'a Settings,
    context: &'a TaskServiceLoggerBuilderContext,
) -> Pin<Box<dyn Future<Output = Result<LoggerRuntime, String>> + Send + 'a>> {
    Box::pin(async move {
        match settings.logger.component {
            LoggerComponent::Database => {
                let mut options =
                    task_service_core::startup::TaskServiceStartup::build_postgres_connect_options(
                        settings
                            .logger
                            .write_connection(&settings.database.connection),
                        &settings.host.database_pool,
                    );
                options.sqlx_logging(false);
                let log_db_pool = Arc::new(
                    sea_orm::Database::connect(options)
                        .await
                        .map_err(|error| format!("日志写连接初始化失败: {error}"))?,
                );

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
                    log_db_pool,
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

/// 构建任务宿主短信发送器
pub fn build_sms_sender(
    settings: &TaskComponentSettings,
    http_clients: Arc<http_client::HttpClientFactory>,
) -> Result<Arc<dyn AppSmsSender>, String> {
    match settings.sms.component {
        SmsComponent::Aliyun => Ok(Arc::new(AliyunSmsSender::new(
            settings.sms.aliyun.clone(),
            http_clients,
        ))),
        SmsComponent::Tencent => Ok(Arc::new(TencentSmsSender::new(
            settings.sms.tencent.clone(),
            http_clients,
        ))),
    }
}

/// 构建任务宿主邮件发送器
pub fn build_email_sender(
    settings: &TaskComponentSettings,
) -> Result<Arc<dyn AppEmailSender>, String> {
    Ok(Arc::new(SmtpEmailSender::new(
        settings.message.smtp.clone(),
    )))
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
            let options =
                task_service_core::startup::TaskServiceStartup::build_postgres_connect_options(
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

/// 队列 Worker 与业务特性共享同一显式模块绑定。
pub fn build_databases<'a>(
    settings: &'a Settings,
    cleanup: &'a mut common::resource_cleanup::ResourceCleanup,
) -> Pin<
    Box<dyn Future<Output = Result<task_service_core::TaskServiceDatabases, String>> + Send + 'a>,
> {
    Box::pin(async move {
        use repository::BusinessModule;
        use repository_core::module::{
            DatabaseModule, DatabaseModules, ModuleConnectionSettings, ModuleDatabase,
        };
        let config = settings
            .database
            .modules
            .get(BusinessModule::NAME)
            .cloned()
            .unwrap_or(ModuleConnectionSettings {
                connection: settings.database.connection.clone(),
                read_connection: settings.database.read_connection.clone(),
            });
        config.validate(BusinessModule::NAME)?;
        let module = ModuleDatabase::<BusinessModule>::connect_managed(
            task_service_core::startup::TaskServiceStartup::build_postgres_connect_options(
                &config.connection,
                &settings.host.database_pool,
            ),
            task_service_core::startup::TaskServiceStartup::build_postgres_connect_options(
                config.read_connection(),
                &settings.host.database_pool,
            ),
            cleanup,
        )
        .await
        .map_err(|error| format!("业务模块连接初始化失败: {error}"))?;
        let mut modules = DatabaseModules::default();
        modules.register(module)?;
        task_service_core::TaskServiceDatabases::for_module::<BusinessModule>(modules)
    })
}

/// 只有模板消息任务需要这些资源，其他宿主可以完全省略该工厂。
#[derive(Clone)]
pub struct MessageResources {
    pub sms_sender: Arc<dyn AppSmsSender>,
    pub email_sender: Arc<dyn AppEmailSender>,
    pub file_storage: Option<Arc<dyn AppFileStorage>>,
    pub content_root: Arc<std::path::PathBuf>,
}
pub fn build_resources<'a>(
    settings: &'a Settings,
    _cleanup: &'a mut common::resource_cleanup::ResourceCleanup,
) -> Pin<Box<dyn Future<Output = Result<common::resources::Resources, String>> + Send + 'a>> {
    Box::pin(async move {
        let components = TaskComponentSettings::load(settings)?;
        let clients = Arc::new(
            http_client::HttpClientFactory::new()
                .map_err(|error| format!("HTTP 客户端工厂初始化失败: {error}"))?,
        );
        let message = MessageResources {
            sms_sender: build_sms_sender(&components, clients.clone())?,
            email_sender: build_email_sender(&components)?,
            file_storage: build_file_storage(&components, clients)?,
            content_root: Arc::new(
                std::env::current_dir()
                    .map_err(|error| format!("读取宿主内容目录失败: {error}"))?,
            ),
        };
        let mut resources = common::resources::Resources::default();
        resources.insert(message)?;
        Ok(resources)
    })
}
