#![allow(clippy::type_complexity)]

/// 宿主运行时装配层
pub mod runtime;

/// 宿主业务特性抽象
pub mod features;

/// 兼容导出宿主运行时模块
pub use runtime::{app_services, builder, settings, state};

use self::builder::WebApiHostBuilder;
use self::features::WebApiFeature;
use self::settings::Settings;
use application_interface::email::AppEmailSender;
use application_interface::storage::AppFileStorage;
use common::identifier_generator::IdentifierGenerator;
use distributed_cache::DistributedCache;
use distributed_lock::DistributedLock;
use llm::LlmClientFactory;
use logger::LoggerRuntime;
use sea_orm::DatabaseConnection;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// WebAPI 分布式锁构建方法
pub type BuildWebApiLock = for<'a> fn(
    &'a Settings,
) -> Pin<
    Box<dyn Future<Output = Result<Arc<dyn DistributedLock>, String>> + Send + 'a>,
>;

/// WebAPI 缓存构建方法
pub type BuildWebApiCache = for<'a> fn(
    &'a Settings,
) -> Pin<
    Box<dyn Future<Output = Result<Arc<dyn DistributedCache>, String>> + Send + 'a>,
>;

/// WebAPI 日志构建上下文
pub struct WebApiLoggerBuilderContext {
    /// 数据库连接池
    pub db_pool: Arc<DatabaseConnection>,

    /// 标识生成器
    pub identifier_generator: IdentifierGenerator,
}

/// WebAPI 日志运行时构建方法
pub type BuildWebApiLoggerRuntime =
    fn(&Settings, &WebApiLoggerBuilderContext) -> Result<LoggerRuntime, String>;

/// WebAPI 宿主配置档案
pub struct WebApiHostProfile {
    /// 宿主名称
    pub host_name: &'static str,

    /// 默认配置路径
    pub default_config_path: &'static str,

    /// 权限模块名
    pub permission_module: &'static str,

    /// 默认锁前缀
    pub default_lock_prefix: &'static str,

    /// 默认日志项目名
    pub default_logger_project: &'static str,

    /// 默认文件根目录
    pub default_file_root: &'static str,

    /// LLM 客户端工厂构建方法
    pub build_llm_client_factory: fn(&Settings) -> Result<LlmClientFactory, String>,

    /// 分布式锁构建方法
    pub build_distributed_lock: BuildWebApiLock,

    /// 缓存构建方法
    pub build_cache: BuildWebApiCache,

    /// 日志运行时构建方法
    pub build_logger_runtime: BuildWebApiLoggerRuntime,

    /// 文件存储构建方法
    pub build_file_storage: fn(&Settings) -> Result<Option<Arc<dyn AppFileStorage>>, String>,

    /// 邮件发送器构建方法
    pub build_email_sender: fn(&Settings) -> Result<Arc<dyn AppEmailSender>, String>,
}

/// 启动指定档案的 WebAPI 宿主
pub async fn run_webapi_host(
    profile: &WebApiHostProfile,
    features: Vec<Box<dyn WebApiFeature>>,
) -> Result<(), String> {
    let settings = Settings::load(profile);
    let llm_client_factory = (profile.build_llm_client_factory)(&settings)?;
    let file_storage = (profile.build_file_storage)(&settings)?;
    let email_sender = (profile.build_email_sender)(&settings)?;
    let host = WebApiHostBuilder::new(
        settings,
        llm_client_factory,
        file_storage,
        email_sender,
        profile.build_distributed_lock,
        profile.build_cache,
        profile.build_logger_runtime,
    )
    .with_features(features)
    .build()
    .await?;

    host.run().await
}
