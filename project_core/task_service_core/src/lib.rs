/// 任务宿主轮询能力
pub mod runner;

/// 任务注册表
pub mod registry;

/// TaskService 应用服务适配
pub mod app_services;

/// TaskService 宿主构建器
pub mod builder;

/// TaskService 业务特性抽象
pub mod features;

/// TaskService 宿主配置
pub mod settings;

/// 任务定义同步能力
pub mod sync;

/// 任务宿主启动帮助方法
pub mod startup;

use application_interface::email::AppEmailSender;
use application_interface::sms::AppSmsSender;
use application_interface::storage::AppFileStorage;
use common::identifier_generator::IdentifierGenerator;
use distributed_cache::DistributedCache;
use distributed_lock::DistributedLock;
use logger::LoggerRuntime;
use sea_orm::DatabaseConnection;
use std::{future::Future, pin::Pin, sync::Arc};

use crate::settings::Settings;

/// TaskService 分布式锁构建方法
pub type BuildTaskServiceLock = for<'a> fn(
    &'a Settings,
) -> Pin<
    Box<dyn Future<Output = Result<Arc<dyn DistributedLock>, String>> + Send + 'a>,
>;

/// TaskService 分布式缓存构建方法
pub type BuildTaskServiceCache = for<'a> fn(
    &'a Settings,
) -> Pin<
    Box<dyn Future<Output = Result<Arc<dyn DistributedCache>, String>> + Send + 'a>,
>;

/// TaskService 短信发送器构建方法
pub type BuildTaskServiceSmsSender = fn(&Settings) -> Result<Arc<dyn AppSmsSender>, String>;

/// TaskService 邮件发送器构建方法
pub type BuildTaskServiceEmailSender = fn(&Settings) -> Result<Arc<dyn AppEmailSender>, String>;

/// TaskService 文件存储构建方法
pub type BuildTaskServiceFileStorage =
    fn(&Settings) -> Result<Option<Arc<dyn AppFileStorage>>, String>;

/// TaskService 日志构建上下文
pub struct TaskServiceLoggerBuilderContext {
    /// 数据库连接池
    pub db_pool: Arc<DatabaseConnection>,

    /// 标识生成器
    pub identifier_generator: IdentifierGenerator,
}

/// TaskService 日志运行时构建方法
pub type BuildTaskServiceLoggerRuntime =
    fn(&Settings, &TaskServiceLoggerBuilderContext) -> Result<LoggerRuntime, String>;
