use async_trait::async_trait;
use repository::BusinessModule;
use repository_core::module::{
    DatabaseModule, DatabaseModules, ModuleConnectionSettings, ModuleDatabase,
};
use repository_core::{ReadDatabaseConnection, database_health};
use sea_orm::{Database, DatabaseConnection};
use std::{future::Future, pin::Pin, sync::Arc};
use webapi_core::{
    health::{HEALTH_STATUS_HEALTHY, HEALTH_STATUS_UNHEALTHY, HealthComponentResponse},
    host::{
        resources::{ResourceHealthCheck, WebApiResources},
        settings::Settings,
    },
    startup::WebApiStartup,
};

pub struct BusinessResources {
    pub modules: DatabaseModules,
    pub log_db_pool: Option<Arc<DatabaseConnection>>,
    pub log_query: logger_database::query::LogQueryService,
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

pub fn build_resources(
    settings: &Settings,
    check_log_read_health: bool,
) -> Pin<Box<dyn Future<Output = Result<WebApiResources, String>> + Send + '_>> {
    build_resources_with_factory(
        settings,
        check_log_read_health,
        crate::BusinessServiceFactory(crate::default_services),
    )
}

pub fn build_resources_with_factory(
    settings: &Settings,
    check_log_read_health: bool,
    factory: crate::BusinessServiceFactory,
) -> Pin<Box<dyn Future<Output = Result<WebApiResources, String>> + Send + '_>> {
    build_resources_with_modules(
        settings,
        check_log_read_health,
        factory,
        DatabaseModules::default(),
    )
}

/// 额外模块由宿主显式连接并注册；这里仅补齐模板 BusinessModule，拒绝重复身份。
pub fn build_resources_with_modules(
    settings: &Settings,
    check_log_read_health: bool,
    factory: crate::BusinessServiceFactory,
    mut modules: DatabaseModules,
) -> Pin<Box<dyn Future<Output = Result<WebApiResources, String>> + Send + '_>> {
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
        let database = ModuleDatabase::<BusinessModule>::connect(
            WebApiStartup::build_postgres_connect_options(
                &config.connection,
                &settings.host.database_pool,
            ),
            WebApiStartup::build_postgres_connect_options(
                config.read_connection(),
                &settings.host.database_pool,
            ),
        )
        .await
        .map_err(|error| format!("业务模块连接初始化失败: {error}"))?;
        modules.register(database)?;

        let log_db_pool = if settings.logger.component == logger::LoggerComponent::Database {
            let mut options = WebApiStartup::build_postgres_connect_options(
                settings
                    .logger
                    .write_connection(&settings.database.connection),
                &settings.host.database_pool,
            );
            // 日志数据库自身的 SQL 不回流日志管线，避免递归记录。
            options.sqlx_logging(false);
            Some(Arc::new(
                Database::connect(options)
                    .await
                    .map_err(|error| format!("日志写连接初始化失败: {error}"))?,
            ))
        } else {
            None
        };

        let log_health_db_pool = match &log_db_pool {
            Some(db) => db.clone(),
            None => {
                let mut options = WebApiStartup::build_postgres_connect_options(
                    settings
                        .logger
                        .write_connection(&settings.database.connection),
                    &settings.host.database_pool,
                );
                options
                    .sqlx_logging(false)
                    .min_connections(0)
                    .connect_lazy(true);
                Arc::new(
                    Database::connect(options)
                        .await
                        .map_err(|error| format!("日志健康检查连接配置失败: {error}"))?,
                )
            }
        };
        let mut log_read_options = WebApiStartup::build_postgres_connect_options(
            settings
                .logger
                .query_connection(&settings.database.connection),
            &settings.host.database_pool,
        );
        log_read_options.sqlx_logging(false);
        if !check_log_read_health {
            log_read_options.min_connections(0).connect_lazy(true);
        }
        let log_query = logger_database::query::LogQueryService::new(Arc::new(
            ReadDatabaseConnection::connect(log_read_options)
                .await
                .map_err(|error| format!("日志只读连接初始化失败: {error}"))?,
        ));

        let mut resources = WebApiResources::default();
        resources.insert(factory)?;
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
            resources.add_health_check(Arc::new(LogReadCheck(log_query.clone())))?;
        }
        resources.insert(BusinessResources {
            modules,
            log_db_pool,
            log_query,
        })?;
        Ok(resources)
    })
}

struct LogReadCheck(logger_database::query::LogQueryService);
#[async_trait]
impl ResourceHealthCheck for LogReadCheck {
    fn name(&self) -> &str {
        "Log.Read"
    }
    async fn check(&self, stopping: &event_bus::CancellationToken) -> HealthComponentResponse {
        let result =
            database_health::check_database_health_with_cancellation(self.0.connection(), stopping)
                .await;
        HealthComponentResponse {
            name: self.name().into(),
            status: if result.is_ok() {
                HEALTH_STATUS_HEALTHY
            } else {
                HEALTH_STATUS_UNHEALTHY
            },
            detail: result.err().map(|e| e.to_string()).unwrap_or_default(),
        }
    }
}

impl BusinessResources {
    pub fn business(&self) -> &ModuleDatabase<BusinessModule> {
        self.modules
            .get()
            .expect("默认业务工厂必须装配 BusinessModule")
    }
}
