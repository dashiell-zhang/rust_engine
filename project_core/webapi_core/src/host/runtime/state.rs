use crate::auth::{HasJwtCodec, JwtDecodingState, TokenRenewalState};
use crate::health::{
    HEALTH_STATUS_HEALTHY, HEALTH_STATUS_UNHEALTHY, HealthComponentResponse, HealthState,
    HealthzResponse,
};
use crate::host::features::WebApiRoutePermission;
use crate::host::settings::Settings;
use application_interface::email::AppEmailSender;
use application_interface::http_client::HttpClientFactory;
use application_interface::storage::AppFileStorage;
use application_interface::{auth::EcJwtCodec, error::AppError};
use async_trait::async_trait;
use chrono::Utc;
use common::app_error::AppError as ApplicationError;
use distributed_cache::{CacheEntryOptions, DistributedCache, DistributedCacheExt};
use distributed_lock::DistributedLock;
use identifier_generator::IdentifierGenerator;
use repository_core::{ReadDatabaseConnection, database_health};
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use std::time::Duration;

/// WebAPI 应用状态
pub struct AppState {
    pub application: Arc<dyn super::super::application::WebApiApplication>,
    pub event_publisher: event_bus::EventPublisher,
    /// 日志专用查询入口，与业务读库独立。
    pub log_query: logger_database::query::LogQueryService,

    /// 日志写入器使用的连接（文件日志组件为空，健康探测单独装配）。
    pub log_db_pool: Option<Arc<DatabaseConnection>>,

    /// 独立日志目标健康探测；不要求启用数据库日志写入器。
    pub log_health_db_pool: Arc<DatabaseConnection>,
    /// 由宿主档案声明是否检查日志读库。
    pub check_log_read_health: bool,

    /// 数据库连接池
    pub db_pool: Arc<DatabaseConnection>,

    /// 数据库只读连接池
    pub read_db_pool: Arc<ReadDatabaseConnection>,

    /// ID 生成器适配器
    pub id_generator: IdentifierGenerator,

    /// 宿主共享分布式锁
    pub distributed_lock: Arc<dyn DistributedLock>,

    /// 文件存储
    pub file_storage: Option<Arc<dyn AppFileStorage>>,

    /// 应用层缓存
    pub cache: Arc<dyn DistributedCache>,

    /// 邮件发送器
    pub email_sender: Arc<dyn AppEmailSender>,

    /// 共享 HTTP 客户端工厂
    pub http_client_factory: Arc<HttpClientFactory>,

    /// 应用配置
    pub settings: Settings,

    /// 当前宿主需要同步的权限路由清单
    pub permission_routes: Vec<WebApiRoutePermission>,

    /// JWT 编解码器
    pub jwt_codec: EcJwtCodec,
}

impl AppState {
    /// 获取主库连接
    pub fn primary_db(&self) -> &DatabaseConnection {
        self.db_pool.as_ref()
    }

    /// 获取只读数据库连接
    pub fn read_db(&self) -> &ReadDatabaseConnection {
        self.read_db_pool.as_ref()
    }
}

impl JwtDecodingState for AppState {
    /// 获取 JWT 验签密钥
    fn jwt_decoding_key(&self) -> &jsonwebtoken::DecodingKey {
        self.jwt_codec.jwt_decoding_key()
    }

    /// 获取 JWT 颁发者
    fn jwt_issuer(&self) -> &str {
        &self.settings.jwt.issuer
    }

    /// 获取 JWT 受众
    fn jwt_audience(&self) -> &str {
        &self.settings.jwt.audience
    }
}

impl HasJwtCodec for AppState {
    /// 获取 JWT 编解码器
    fn jwt_codec(&self) -> &EcJwtCodec {
        &self.jwt_codec
    }
}

#[async_trait]
impl TokenRenewalState for AppState {
    /// 委托授权服务执行 Token 续签逻辑
    async fn try_renew_token(
        &self,
        user_id: i64,
        token_id: i64,
        nbf: i64,
        exp: i64,
    ) -> Result<Option<String>, ApplicationError> {
        self.application
            .try_renew_token(self, user_id, token_id, nbf, exp)
            .await
    }
}

#[async_trait]
impl HealthState for AppState {
    /// 并行检查源宿主注册的数据库、缓存和事件目标。
    async fn health_report(&self) -> HealthzResponse {
        self.health_report_with_cancellation(&event_bus::CancellationToken::new())
            .await
    }
    async fn health_report_with_cancellation(
        &self,
        stopping: &event_bus::CancellationToken,
    ) -> HealthzResponse {
        let (write, read, log_write, log_read, cache, event) = tokio::join!(
            check_named_database_health(self.primary_db(), "Business.Write", stopping),
            check_named_database_health(self.read_db_pool.as_ref(), "Business.Read", stopping),
            check_named_database_health(self.log_health_db_pool.as_ref(), "Log.Write", stopping),
            async {
                if self.check_log_read_health {
                    Some(
                        check_named_database_health(
                            self.log_query.connection(),
                            "Log.Read",
                            stopping,
                        )
                        .await,
                    )
                } else {
                    None
                }
            },
            check_cache_health(self, stopping),
            async {
                health_result(
                    "EventBus",
                    self.event_publisher
                        .health_with_cancellation(stopping)
                        .await,
                )
            },
        );
        let mut components = vec![write, read, log_write, cache, event];
        components.extend(log_read);
        let status = if components
            .iter()
            .all(|component| component.status == HEALTH_STATUS_HEALTHY)
        {
            HEALTH_STATUS_HEALTHY
        } else {
            HEALTH_STATUS_UNHEALTHY
        };
        HealthzResponse {
            service: self.settings.logger.project.clone(),
            status,
            checked_at: Utc::now().to_rfc3339(),
            components,
        }
    }
}

async fn check_named_database_health<C: database_health::DatabaseHealthConnection>(
    db: &C,
    name: &str,
    stopping: &event_bus::CancellationToken,
) -> HealthComponentResponse {
    health_result(
        name,
        database_health::check_database_health_with_cancellation(db, stopping)
            .await
            .map_err(|error| error.to_string()),
    )
}

async fn check_cache_health(
    state: &AppState,
    stopping: &event_bus::CancellationToken,
) -> HealthComponentResponse {
    let check = state.cache.set_string_with_options(
        "cacheHealthCheck",
        "",
        CacheEntryOptions::relative(Duration::from_secs(10)),
    );
    let result = tokio::select! {
        biased;
        _ = stopping.cancelled() => Err("健康检查已取消".into()),
        result = check => result.map_err(|error| error.to_string()),
    };
    health_result("Business.Cache", result)
}

fn health_result(name: &str, result: Result<(), String>) -> HealthComponentResponse {
    match result {
        Ok(()) => HealthComponentResponse {
            name: name.to_owned(),
            status: HEALTH_STATUS_HEALTHY,
            detail: String::new(),
        },
        Err(detail) => HealthComponentResponse {
            name: name.to_owned(),
            status: HEALTH_STATUS_UNHEALTHY,
            detail,
        },
    }
}

impl application_interface::events::EventPublisherProvider for AppState {
    fn event_publisher(&self) -> &event_bus::EventPublisher {
        &self.event_publisher
    }
}

#[async_trait]
impl crate::auth::PermissionState for AppState {
    async fn check_permission(
        &self,
        current_user: application_interface::context::CurrentUser,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<bool, AppError> {
        self.application
            .check_permission(self, current_user, module, route, allow_unbound)
            .await
    }
}
impl AppState {
    pub async fn sync_permission_routes(&self) -> Result<bool, AppError> {
        self.application.sync_permission_routes(self).await
    }
}
