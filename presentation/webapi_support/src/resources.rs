use async_trait::async_trait;
use repository::BusinessModule;
use repository_core::module::{
    DatabaseModule, DatabaseModules, ModuleConnectionSettings, ModuleDatabase,
};
use repository_core::{ReadDatabaseConnection, database_health};
use sea_orm::{Database, DatabaseConnection};
use std::{future::Future, pin::Pin, sync::Arc};
use webapi_core::host::resources::ResourceCleanup;
use webapi_core::{
    health::{HEALTH_STATUS_HEALTHY, HEALTH_STATUS_UNHEALTHY, HealthComponentResponse},
    host::{
        resources::{ResourceHealthCheck, WebApiResources},
        settings::Settings,
    },
};

/// 显式选择本地存储时内部为 None；没有注册资源则属于缺少装配。
pub struct FileStorageResource(pub Option<Arc<dyn file_storage::AppFileStorage>>);

pub struct BusinessResources {
    pub modules: DatabaseModules,
    pub log_db_pool: Option<Arc<DatabaseConnection>>,
}

struct DatabaseCheck<C> {
    name: String,
    connection: Arc<C>,
}
#[async_trait]
impl<C: database_health::DatabaseHealthConnection + Send + Sync> ResourceHealthCheck
    for DatabaseCheck<C>
{
    fn name(&self) -> &str {
        &self.name
    }
    async fn check(&self, stopping: &event_bus::CancellationToken) -> HealthComponentResponse {
        let result = database_health::check_database_health_with_cancellation(
            self.connection.as_ref(),
            stopping,
        )
        .await;
        HealthComponentResponse {
            name: self.name.clone(),
            status: if result.is_ok() {
                HEALTH_STATUS_HEALTHY
            } else {
                HEALTH_STATUS_UNHEALTHY
            },
            detail: result.err().map(|e| e.to_string()).unwrap_or_default(),
        }
    }
}

/// 只创建业务数据库、日志写连接与健康检查；服务能力由入口分别注册。
pub fn build_resources<'a>(
    settings: &'a Settings,
    check_log_read_health: bool,
    cleanup: &'a mut ResourceCleanup,
) -> Pin<Box<dyn Future<Output = Result<WebApiResources, String>> + Send + 'a>> {
    build_resources_with_modules(
        settings,
        check_log_read_health,
        DatabaseModules::default(),
        cleanup,
    )
}
pub fn build_resources_with_modules<'a>(
    settings: &'a Settings,
    check_log_read_health: bool,
    mut modules: DatabaseModules,
    cleanup: &'a mut ResourceCleanup,
) -> Pin<Box<dyn Future<Output = Result<WebApiResources, String>> + Send + 'a>> {
    Box::pin(async move {
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
        let database = ModuleDatabase::<BusinessModule>::connect_managed(
            repository_core::connection::build_postgres_connect_options(
                &config.connection,
                &settings.host.database_pool,
            ),
            repository_core::connection::build_postgres_connect_options(
                config.read_connection(),
                &settings.host.database_pool,
            ),
            cleanup,
        )
        .await
        .map_err(|error| format!("业务模块连接初始化失败: {error}"))?;
        modules.register(database)?;

        let log_db_pool = if settings.logger.component == logger::LoggerComponent::Database {
            let mut options = repository_core::connection::build_postgres_connect_options(
                settings
                    .logger
                    .write_connection(&settings.database.connection),
                &settings.host.database_pool,
            );
            // 日志数据库自身的 SQL 不回流日志管线，避免递归记录。
            options.sqlx_logging(false);
            Some(
                cleanup
                    .acquire(
                        "Log.Write",
                        async {
                            Database::connect(options)
                                .await
                                .map(Arc::new)
                                .map_err(|error| format!("日志写连接初始化失败: {error}"))
                        },
                        |pool, _| async move {
                            pool.close_by_ref().await.map_err(|error| error.to_string())
                        },
                    )
                    .await?,
            )
        } else {
            None
        };

        let log_health_db_pool = match &log_db_pool {
            Some(db) => db.clone(),
            None => {
                let mut options = repository_core::connection::build_postgres_connect_options(
                    settings
                        .logger
                        .write_connection(&settings.database.connection),
                    &settings.host.database_pool,
                );
                options
                    .sqlx_logging(false)
                    .min_connections(0)
                    .connect_lazy(true);
                cleanup
                    .acquire(
                        "Log.Health",
                        async {
                            Database::connect(options)
                                .await
                                .map(Arc::new)
                                .map_err(|error| format!("日志健康检查连接配置失败: {error}"))
                        },
                        |pool, _| async move {
                            pool.close_by_ref().await.map_err(|error| error.to_string())
                        },
                    )
                    .await?
            }
        };
        let mut resources = WebApiResources::default();
        for module in modules.connections() {
            let prefix = if module.name == BusinessModule::NAME {
                "Business"
            } else {
                module.name
            };
            resources.add_health_check(Arc::new(DatabaseCheck {
                name: format!("{prefix}.Write"),
                connection: module.write.clone(),
            }))?;
            resources.add_health_check(Arc::new(DatabaseCheck {
                name: format!("{prefix}.Read"),
                connection: module.read.clone(),
            }))?;
        }
        resources.add_health_check(Arc::new(DatabaseCheck {
            name: "Log.Write".into(),
            connection: log_health_db_pool,
        }))?;
        if check_log_read_health {
            let connection = build_log_read_connection(settings, cleanup).await?;
            resources.add_health_check(Arc::new(DatabaseCheck {
                name: "Log.Read".into(),
                connection: connection.clone(),
            }))?;
            resources.insert(LogReadConnection(connection))?;
        }
        resources.insert(BusinessResources {
            modules,
            log_db_pool,
        })?;
        Ok(resources)
    })
}

impl BusinessResources {
    pub fn business(&self) -> &ModuleDatabase<BusinessModule> {
        self.modules
            .get()
            .expect("默认业务工厂必须装配 BusinessModule")
    }
}

/// 查询能力与数据库健康检查分别选择；客户端无需创建查询服务。
pub struct LogQueryResource(pub logger_database::query::LogQueryService);
struct LogReadConnection(Arc<ReadDatabaseConnection>);
async fn build_log_read_connection(
    settings: &Settings,
    cleanup: &mut ResourceCleanup,
) -> Result<Arc<ReadDatabaseConnection>, String> {
    let mut options = repository_core::connection::build_postgres_connect_options(
        settings
            .logger
            .query_connection(&settings.database.connection),
        &settings.host.database_pool,
    );
    options.sqlx_logging(false);
    cleanup
        .acquire(
            "Log.Read",
            async {
                ReadDatabaseConnection::connect(options)
                    .await
                    .map(Arc::new)
                    .map_err(|error| format!("日志只读连接初始化失败: {error}"))
            },
            |pool, _| async move { pool.close().await.map_err(|error| error.to_string()) },
        )
        .await
}
pub async fn register_log_query(
    resources: &mut WebApiResources,
    settings: &Settings,
    cleanup: &mut ResourceCleanup,
) -> Result<(), String> {
    let connection = match resources.get::<LogReadConnection>() {
        Ok(connection) => connection.0.clone(),
        Err(_) => {
            let connection = build_log_read_connection(settings, cleanup).await?;
            resources.insert(LogReadConnection(connection.clone()))?;
            connection
        }
    };
    resources.insert(LogQueryResource(
        logger_database::query::LogQueryService::new(connection),
    ))
}
