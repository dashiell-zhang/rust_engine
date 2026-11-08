use crate::auth::{HasJwtCodec, JwtDecodingState, TokenRenewalState};
use crate::health::{
    HEALTH_STATUS_HEALTHY, HEALTH_STATUS_UNHEALTHY, HealthComponentResponse, HealthState,
    HealthzResponse,
};
use crate::host::features::WebApiRoutePermission;
use crate::host::settings::Settings;
use application_interface::error::AppError;
use application_runtime::auth::EcJwtCodec;
use async_trait::async_trait;
use chrono::Utc;
use common::app_error::AppError as ApplicationError;
use distributed_cache::{CacheEntryOptions, DistributedCache, DistributedCacheExt};
use distributed_lock::DistributedLock;
use email::AppEmailSender;
use file_storage::AppFileStorage;
use http_client::HttpClientFactory;
use identifier_generator::IdentifierGenerator;
use std::sync::Arc;
use std::time::Duration;

/// WebAPI 应用状态
pub struct AppState {
    pub application: Arc<dyn super::super::application::WebApiApplication>,
    pub event_publisher: event_bus::EventPublisher,
    pub resources: crate::host::resources::WebApiResources,

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
        let (mut components, cache, event) = tokio::join!(
            self.resources.check_health(stopping),
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
        components.extend([cache, event]);
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
        current_user: application_runtime::context::CurrentUser,
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
