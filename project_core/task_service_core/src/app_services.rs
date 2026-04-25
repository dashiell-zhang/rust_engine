use application_interface::context::AppContext;
use identifier_generator::IdentifierGenerator;
use sea_orm::DatabaseConnection;
use source_generator_runtime::generate_direct_service_factory;

/// TaskService 应用服务入口
pub struct TaskServiceApplicationAdapter<'a> {
    /// 数据库连接
    db_pool: &'a DatabaseConnection,

    /// ID 生成器
    identifier_generator: &'a IdentifierGenerator,
}

impl<'a> TaskServiceApplicationAdapter<'a> {
    /// 创建 TaskService 应用服务适配器
    pub fn new(
        db_pool: &'a DatabaseConnection,
        identifier_generator: &'a IdentifierGenerator,
    ) -> Self {
        Self {
            db_pool,
            identifier_generator,
        }
    }

    /// 获取主库应用层通用上下文
    pub fn primary_app_context(&self) -> AppContext<'_> {
        AppContext {
            db: self.db_pool,
            id_generator: self.identifier_generator,
            // 任务宿主的系统流程不绑定登录用户
            current_user: None,
        }
    }

    /// 获取应用层通用上下文
    pub fn app_context(&self) -> AppContext<'_> {
        self.primary_app_context()
    }
}

generate_direct_service_factory! {
    impl_generics = <'a>;
    factory = TaskServiceApplicationAdapter<'a>;

    (self.primary_app_context()) {
        system_task: application_service_task_service::system_task::service::SystemTaskService<'_>
    }
}
