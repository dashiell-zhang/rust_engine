use crate::auth::{HasJwtCodec, JwtDecodingState, TokenRenewalState, TokenValidationState};
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
use application_service::authorize::service::{
    build_token_valid_cache_key, try_cache_token_validity,
};
use async_trait::async_trait;
use chrono::Utc;
use common::app_error::AppError as ApplicationError;
use distributed_cache::{CacheEntryOptions, DistributedCache, DistributedCacheExt};
use distributed_lock::DistributedLock;
use identifier_generator::IdentifierGenerator;
use repository::{database_health, extensions::SoftDeleteQueryExt, user_token};
use sea_orm::DatabaseConnection;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::sync::Arc;
use std::time::Duration;

/// WebAPI 应用状态
pub struct AppState {
    /// 数据库连接池
    pub db_pool: Arc<DatabaseConnection>,

    /// ID 生成器适配器
    pub id_generator: IdentifierGenerator,

    /// 角色编码并发校验锁
    pub role_code_lock: Arc<dyn DistributedLock>,

    /// 文件存储
    pub file_storage: Option<Arc<dyn AppFileStorage>>,

    /// 应用层缓存
    pub cache: Arc<dyn DistributedCache>,

    /// 邮件发送器
    pub email_sender: Arc<dyn AppEmailSender>,

    /// 共享 HTTP 客户端工厂
    pub http_client_factory: HttpClientFactory,

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
impl TokenValidationState for AppState {
    /// 校验令牌是否仍然有效
    async fn validate_token(&self, user_id: i64, token_id: i64) -> Result<bool, ApplicationError> {
        let cache_key = build_token_valid_cache_key(user_id, token_id);

        if let Some(cached_value) = self.cache.get_string(&cache_key).await.map_err(|error| {
            ApplicationError::internal(format!("Token 状态缓存读取失败: {error}"))
        })? {
            return match cached_value.as_str() {
                "1" => Ok(true),
                "0" => Ok(false),
                _ => Err(ApplicationError::internal("Token 状态缓存值格式无效")),
            };
        }

        let token_exists = user_token::Entity::find_by_id(token_id)
            .filter(user_token::Column::UserId.eq(user_id))
            .not_deleted(user_token::Column::DeleteTime)
            .one(self.db_pool.as_ref())
            .await
            .map_err(|error| ApplicationError::internal(format!("Token 状态校验失败: {error}")))?
            .is_some();

        try_cache_token_validity(Some(self.cache.as_ref()), user_id, token_id, token_exists).await;

        Ok(token_exists)
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
        self.authorize()
            .try_renew_token(user_id, token_id, nbf, exp)
            .await
    }
}

#[async_trait]
impl HealthState for AppState {
    /// 聚合数据库 缓存和分布式锁的健康状态
    async fn health_report(&self) -> HealthzResponse {
        let mut components = Vec::with_capacity(3);

        components.push(check_database_health(self).await);

        components.push(check_cache_health(self).await);
        components.push(check_lock_health(self).await);

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

/// 检查数据库健康状态
async fn check_database_health(state: &AppState) -> HealthComponentResponse {
    check_named_database_health(
        state.primary_db(),
        "database",
        format!(
            "provider=postgresql target=primary max_connections={} min_connections={}",
            state.settings.host.database_pool.max_connections,
            state.settings.host.database_pool.min_connections
        ),
    )
    .await
}

/// 检查命名数据库连接健康状态
async fn check_named_database_health(
    db: &DatabaseConnection,
    name: &str,
    detail: String,
) -> HealthComponentResponse {
    match database_health::check_database_health(db).await {
        Ok(_) => HealthComponentResponse {
            name: name.to_string(),
            status: HEALTH_STATUS_HEALTHY,
            detail,
        },
        Err(error) => HealthComponentResponse {
            name: name.to_string(),
            status: HEALTH_STATUS_UNHEALTHY,
            detail: format!("{detail} error={error}"),
        },
    }
}

/// 检查缓存健康状态
async fn check_cache_health(state: &AppState) -> HealthComponentResponse {
    let cache_component = cache_component_name(&state.settings);
    let key = format!(
        "healthz:cache:{}:{}",
        state.settings.logger.project,
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let value = "ok";

    // 通过写入 读取和删除验证缓存基本读写能力
    let result = async {
        state
            .cache
            .set_string_with_options(
                &key,
                value,
                CacheEntryOptions::relative(Duration::from_secs(30)),
            )
            .await?;
        let cached_value = state.cache.get_string(&key).await?;
        state.cache.remove(&key).await?;
        Ok::<Option<String>, AppError>(cached_value)
    }
    .await;

    match result {
        Ok(Some(cached_value)) if cached_value == value => HealthComponentResponse {
            name: "cache".to_string(),
            status: HEALTH_STATUS_HEALTHY,
            detail: format!("component={cache_component}"),
        },
        Ok(other_value) => HealthComponentResponse {
            name: "cache".to_string(),
            status: HEALTH_STATUS_UNHEALTHY,
            detail: format!("component={cache_component} unexpected_value={other_value:?}"),
        },
        Err(error) => HealthComponentResponse {
            name: "cache".to_string(),
            status: HEALTH_STATUS_UNHEALTHY,
            detail: format!("component={cache_component} error={}", error.message()),
        },
    }
}

/// 检查分布式锁健康状态
async fn check_lock_health(state: &AppState) -> HealthComponentResponse {
    let lock_component = lock_component_name(&state.settings);
    let key = format!(
        "healthz:lock:{}:{}",
        state.settings.logger.project,
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );

    // 通过临时锁申请验证当前锁组件可用性
    match state
        .role_code_lock
        .try_lock(&key, Duration::from_secs(5), 1)
        .await
    {
        Ok(Some(guard)) => {
            drop(guard);
            HealthComponentResponse {
                name: "lock".to_string(),
                status: HEALTH_STATUS_HEALTHY,
                detail: format!("component={lock_component}"),
            }
        }
        Ok(None) => HealthComponentResponse {
            name: "lock".to_string(),
            status: HEALTH_STATUS_UNHEALTHY,
            detail: format!("component={lock_component} detail=lock_not_acquired"),
        },
        Err(error) => HealthComponentResponse {
            name: "lock".to_string(),
            status: HEALTH_STATUS_UNHEALTHY,
            detail: format!("component={lock_component} error={error}"),
        },
    }
}

/// 获取缓存组件名称
fn cache_component_name(settings: &Settings) -> &'static str {
    match settings.cache.component {
        crate::host::settings::CacheComponent::InMemory => "in_memory",
        crate::host::settings::CacheComponent::Redis => "redis",
    }
}

/// 获取锁组件名称
fn lock_component_name(settings: &Settings) -> &'static str {
    match settings.lock.component {
        crate::host::settings::LockComponent::InMemory => "in_memory",
        crate::host::settings::LockComponent::Redis => "redis",
    }
}
