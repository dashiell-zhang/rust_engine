use crate::registry::TaskRegistry;
use application_interface::email::AppEmailSender;
use application_interface::sms::AppSmsSender;
use application_interface::storage::AppFileStorage;
use common::identifier_generator::IdentifierGenerator;
use repository::read_database::ReadDatabaseConnection;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

/// TaskService 特性注册上下文
#[derive(Clone)]
pub struct TaskServiceFeatureContext {
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

    /// 文件根目录列表
    pub file_root_paths: Arc<Vec<String>>,
}

/// TaskService 业务特性
pub trait TaskServiceFeature: Send + Sync {
    /// 向任务注册表注册当前特性的任务
    fn register(
        &self,
        registry: &mut TaskRegistry,
        context: &TaskServiceFeatureContext,
    ) -> Result<(), String>;
}
