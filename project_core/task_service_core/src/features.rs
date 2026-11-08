use crate::registry::TaskRegistry;
use application_interface::email::AppEmailSender;
use application_interface::sms::AppSmsSender;
use application_interface::storage::AppFileStorage;
use common::identifier_generator::IdentifierGenerator;
use repository_core::ReadDatabaseConnection;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

/// TaskService 特性注册上下文
#[derive(Clone)]
pub struct TaskServiceFeatureContext {
    /// 宿主业务缓存；任务特性按需克隆使用，不参与 ID 节点协调。
    pub cache: Arc<dyn distributed_cache::DistributedCache>,
    pub event_publisher: event_bus::EventPublisher,
    /// 数据库连接
    pub db_pool: Arc<DatabaseConnection>,

    /// 数据库只读连接池
    pub read_db_pool: Arc<ReadDatabaseConnection>,

    /// ID 生成器
    pub identifier_generator: IdentifierGenerator,

    /// 短信发送器
    pub sms_sender: Arc<dyn AppSmsSender>,

    /// 邮件发送器
    pub email_sender: Arc<dyn AppEmailSender>,

    /// 文件存储
    pub file_storage: Option<Arc<dyn AppFileStorage>>,

    /// 宿主启动时的内容目录
    pub content_root: Arc<std::path::PathBuf>,
}

/// TaskService 业务特性
pub trait TaskServiceFeature: Send + Sync {
    fn register_event_listeners(
        &self,
        _registry: &mut event_bus::EventListenerRegistry,
        _context: &TaskServiceFeatureContext,
    ) -> Result<(), String> {
        Ok(())
    }

    /// 向任务注册表注册当前特性的任务
    fn register(
        &self,
        registry: &mut TaskRegistry,
        context: &TaskServiceFeatureContext,
    ) -> Result<(), String>;
}

impl application_interface::events::EventPublisherProvider for TaskServiceFeatureContext {
    fn event_publisher(&self) -> &event_bus::EventPublisher {
        &self.event_publisher
    }
}
