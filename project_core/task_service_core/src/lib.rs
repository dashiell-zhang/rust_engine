/// 任务调用和返回值适配
pub mod task_handler;

mod debug_menu;

/// 任务宿主轮询能力
pub mod runner;

/// 任务注册表
pub mod registry;

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
pub type BuildTaskServiceSmsSender = fn(
    &Settings,
    Arc<application_runtime::http_client::HttpClientFactory>,
) -> Result<Arc<dyn AppSmsSender>, String>;

/// TaskService 邮件发送器构建方法
pub type BuildTaskServiceEmailSender = fn(&Settings) -> Result<Arc<dyn AppEmailSender>, String>;

/// TaskService 文件存储构建方法
pub type BuildTaskServiceFileStorage = fn(
    &Settings,
    Arc<application_runtime::http_client::HttpClientFactory>,
) -> Result<Option<Arc<dyn AppFileStorage>>, String>;

/// TaskService 日志构建上下文
pub struct TaskServiceLoggerBuilderContext {
    /// 独立日志写连接池；本地文件组件为空
    pub log_db_pool: Option<Arc<DatabaseConnection>>,

    /// 标识生成器
    pub identifier_generator: IdentifierGenerator,
}

/// TaskService 日志运行时构建方法
pub type BuildTaskServiceLoggerRuntime =
    for<'a> fn(
        &'a Settings,
        &'a TaskServiceLoggerBuilderContext,
    ) -> Pin<Box<dyn Future<Output = Result<LoggerRuntime, String>> + Send + 'a>>;

/// 事件后端由运行入口选择。
pub type BuildTaskServiceEventTransport =
    fn(&Settings, &IdentifierGenerator) -> Result<Arc<dyn event_bus::EventTransport>, String>;

/// 已选定队列模块的数据库集合；构造时从同一个类型绑定取得读写池。
pub struct TaskServiceDatabases {
    pub modules: Arc<repository_core::module::DatabaseModules>,
    pub(crate) queue_write: Arc<DatabaseConnection>,
    pub(crate) queue_read: Arc<repository_core::ReadDatabaseConnection>,
}
impl TaskServiceDatabases {
    pub fn for_module<M: repository_core::module::DatabaseModule>(
        modules: repository_core::module::DatabaseModules,
    ) -> Result<Self, String> {
        let module = modules.get::<M>()?;
        let queue_write = module.write_pool();
        let queue_read = module.read_pool();
        Ok(Self {
            modules: Arc::new(modules),
            queue_write,
            queue_read,
        })
    }
}
pub type BuildTaskServiceDatabases = for<'a> fn(
    &'a Settings,
) -> Pin<
    Box<dyn Future<Output = Result<TaskServiceDatabases, String>> + Send + 'a>,
>;
