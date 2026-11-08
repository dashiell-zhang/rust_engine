#![allow(clippy::type_complexity)]

/// 宿主运行时装配层
pub mod runtime;

/// 宿主业务特性抽象
pub mod features;

/// 兼容导出宿主运行时模块
pub use runtime::{builder, settings, state};

/// 运行入口提供的业务策略
pub mod application;

use self::builder::WebApiHostBuilder;
use self::features::WebApiFeature;
use self::settings::Settings;
use application_interface::email::AppEmailSender;
use application_interface::storage::AppFileStorage;
use common::identifier_generator::IdentifierGenerator;
use distributed_cache::DistributedCache;
use distributed_lock::DistributedLock;
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
    /// 独立日志写连接池；本地文件组件为空
    pub log_db_pool: Option<Arc<DatabaseConnection>>,

    /// 标识生成器
    pub identifier_generator: IdentifierGenerator,
}

/// WebAPI 日志运行时构建方法
pub type BuildWebApiLoggerRuntime =
    for<'a> fn(
        &'a Settings,
        &'a WebApiLoggerBuilderContext,
    ) -> Pin<Box<dyn Future<Output = Result<LoggerRuntime, String>> + Send + 'a>>;

/// 事件后端由运行入口选择，公共宿主只依赖传输契约。
pub type BuildWebApiEventTransport =
    fn(&Settings, &IdentifierGenerator) -> Result<Arc<dyn event_bus::EventTransport>, String>;

pub type ConfigureWebApiHttpClients = fn(
    &Settings,
    Arc<dyn DistributedCache>,
    &mut application_interface::http_client::HttpClientFactory,
) -> Result<(), String>;

/// 文件组件使用宿主已配置的共享 HTTP 工厂。
pub type BuildWebApiFileStorage = fn(
    &Settings,
    Arc<application_interface::http_client::HttpClientFactory>,
) -> Result<Option<Arc<dyn AppFileStorage>>, String>;

/// WebAPI 宿主配置档案
pub struct WebApiHostProfile {
    pub application: fn() -> Arc<dyn application::WebApiApplication>,
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

    /// 管理端注册日志读库健康检查，客户端不注册。
    pub check_log_read_health: bool,

    /// 是否把文件根目录作为公开 Web 根目录；由管理端显式启用。
    pub serve_web_root: bool,

    /// 分布式锁构建方法
    pub build_distributed_lock: BuildWebApiLock,

    /// 缓存构建方法
    pub build_cache: BuildWebApiCache,

    /// 日志运行时构建方法
    pub build_logger_runtime: BuildWebApiLoggerRuntime,
    pub build_event_transport: BuildWebApiEventTransport,

    /// 文件存储构建方法
    pub build_file_storage: BuildWebApiFileStorage,

    /// 邮件发送器构建方法
    pub build_email_sender: fn(&Settings) -> Result<Arc<dyn AppEmailSender>, String>,
    /// 具体宿主装配命名客户端和出站处理器。
    pub configure_http_clients: Option<ConfigureWebApiHttpClients>,
}

/// 启动指定档案的 WebAPI 宿主
pub async fn run_webapi_host(
    profile: &WebApiHostProfile,
    features: Vec<Box<dyn WebApiFeature>>,
) -> Result<(), String> {
    let settings = Settings::load(profile);
    let email_sender = (profile.build_email_sender)(&settings)?;
    let host = WebApiHostBuilder::new(
        settings,
        profile.build_file_storage,
        email_sender,
        profile.build_distributed_lock,
        profile.build_cache,
        profile.build_logger_runtime,
    )
    .with_application((profile.application)())
    .with_log_read_health(profile.check_log_read_health)
    .with_web_root(profile.serve_web_root)
    .with_event_transport_factory(profile.build_event_transport)
    .with_http_client_configuration(profile.configure_http_clients)
    .with_features(features)
    .build()
    .await?;

    host.run().await
}
