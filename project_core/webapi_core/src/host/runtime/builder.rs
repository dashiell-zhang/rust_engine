use std::{
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::Arc,
};

use crate::api_doc::ApiDoc;
use crate::host::{
    BuildWebApiCache, BuildWebApiLock, BuildWebApiLoggerRuntime, WebApiLoggerBuilderContext,
    features::{WebApiFeature, WebApiRoutePermission},
    settings::{CacheComponent, FileStorageComponent, Settings},
    state::AppState,
};
use crate::rate_limit::RateLimiter;
use crate::{
    builder::WebApiRouterBuilder, openapi::apply_openapi_enhancements, startup::WebApiStartup,
};
use application_interface::auth::EcJwtCodec;
use application_interface::context::AppContext;
use application_interface::email::AppEmailSender;
use application_interface::http_client::HttpClientFactory;
use application_interface::storage::AppFileStorage;
use application_service::permission_route::service::{
    PermissionRouteService, PermissionRouteSyncItem,
};
use axum::Router;
use identifier_generator::IdentifierGenerator;
use identifier_generator::refresh::RefreshSignTask;
use llm::LlmClientFactory;
use logger::LoggerRuntime;
use sea_orm::Database;
use utoipa::OpenApi;

/// WebAPI 宿主构建器
pub struct WebApiHostBuilder {
    /// 宿主配置
    settings: Settings,

    /// LLM 客户端工厂
    llm_client_factory: LlmClientFactory,

    /// 文件存储
    file_storage: Option<Arc<dyn AppFileStorage>>,

    /// 邮件发送器
    email_sender: Arc<dyn AppEmailSender>,

    /// 分布式锁构建方法
    build_distributed_lock: BuildWebApiLock,

    /// 缓存构建方法
    build_cache: BuildWebApiCache,

    /// 日志运行时构建方法
    build_logger_runtime: BuildWebApiLoggerRuntime,

    /// 已注册特性
    features: Vec<Box<dyn WebApiFeature>>,
}

/// WebAPI 宿主实例
pub struct WebApiHost {
    /// 最终路由
    app: Router,

    /// 监听地址
    listen_address: String,

    /// 日志运行时守卫
    _logger_runtime: LoggerRuntime,
}

impl WebApiHostBuilder {
    /// 创建 WebAPI 宿主构建器
    pub fn new(
        settings: Settings,
        llm_client_factory: LlmClientFactory,
        file_storage: Option<Arc<dyn AppFileStorage>>,
        email_sender: Arc<dyn AppEmailSender>,
        build_distributed_lock: BuildWebApiLock,
        build_cache: BuildWebApiCache,
        build_logger_runtime: BuildWebApiLoggerRuntime,
    ) -> Self {
        Self {
            settings,
            llm_client_factory,
            file_storage,
            email_sender,
            build_distributed_lock,
            build_cache,
            build_logger_runtime,
            features: Vec::new(),
        }
    }

    /// 注册单个 WebAPI 特性
    pub fn with_feature(mut self, feature: Box<dyn WebApiFeature>) -> Self {
        self.features.push(feature);
        self
    }

    /// 注册特性集合
    pub fn with_features(mut self, features: Vec<Box<dyn WebApiFeature>>) -> Self {
        for feature in features {
            self = self.with_feature(feature);
        }
        self
    }

    /// 构建 WebAPI 宿主实例
    pub async fn build(self) -> Result<WebApiHost, String> {
        let jwt_key_pair = WebApiStartup::parse_ec_jwt_key_pair(
            &self.settings.jwt.private_key,
            &self.settings.jwt.public_key,
        )?;

        let role_code_lock = (self.build_distributed_lock)(&self.settings).await?;
        let distributed_cache = (self.build_cache)(&self.settings).await?;
        let identifier_generator = IdentifierGenerator::from_infrastructure(
            &self.settings.identifier_generator,
            Arc::clone(&role_code_lock),
            Arc::clone(&distributed_cache),
        )
        .await
        .map_err(|error| format!("ID 服务初始化失败: {error}"))?;
        let should_refresh_identifier_sign = identifier_generator.requires_sign_refresh().await;
        let cache = Arc::clone(&distributed_cache);

        let db_pool = Arc::new(
            Database::connect(WebApiStartup::build_postgres_connect_options(
                self.settings.database.connection.clone(),
                &self.settings.host.database_pool,
            ))
            .await
            .map_err(|error| format!("数据库连接初始化失败: {error}"))?,
        );

        let logger_runtime = (self.build_logger_runtime)(
            &self.settings,
            &WebApiLoggerBuilderContext {
                db_pool: Arc::clone(&db_pool),
                identifier_generator: identifier_generator.clone(),
            },
        )?;

        // 输出关键宿主配置 便于排查启动期依赖装配问题
        tracing::info!(
            target: "app.startup",
            listen_address = %self.settings.host.listen_address,
            permission_module = %self.settings.permission_module,
            lock_component = %lock_component_name(&self.settings),
            cache_component = %cache_component_name(&self.settings),
            logger_component = ?self.settings.logger.component,
            file_component = %file_component_name(&self.settings),
            file_root_path = %self.settings.file.root_path,
            max_connections = self.settings.host.database_pool.max_connections,
            min_connections = self.settings.host.database_pool.min_connections,
            connect_timeout_seconds = self.settings.host.database_pool.connect_timeout_seconds,
            acquire_timeout_seconds = self.settings.host.database_pool.acquire_timeout_seconds,
            idle_timeout_seconds = self.settings.host.database_pool.idle_timeout_seconds,
            max_lifetime_seconds = self.settings.host.database_pool.max_lifetime_seconds,
            "WebAPI 宿主依赖初始化完成"
        );

        let id_generator = identifier_generator.clone();

        let permission_routes = self
            .features
            .iter()
            .flat_map(|feature| feature.permission_routes())
            .collect::<Vec<WebApiRoutePermission>>();

        let permission_route_sync_items = permission_routes
            .iter()
            .map(|item| PermissionRouteSyncItem {
                route: item.route,
                remark: item.remark,
            })
            .collect::<Vec<_>>();

        PermissionRouteService::new(AppContext {
            db: db_pool.as_ref(),
            id_generator: &id_generator,
            // 启动阶段同步权限路由时尚未进入请求上下文
            current_user: None,
        })
        .sync_routes(
            &self.settings.permission_module,
            &permission_route_sync_items,
        )
        .await
        .map_err(|error| error.message().to_string())?;

        // 提前取出限流配置，避免 settings move 进 AppState 后无法访问
        let rate_limit_settings = self.settings.rate_limit.clone();
        let cors_settings = self.settings.cors.clone();

        tracing::info!(target: "app.startup", "权限路由同步完成，开始构建 AppState");

        let app_state = Arc::new(AppState {
            db_pool,
            id_generator,
            role_code_lock,
            file_storage: self.file_storage,
            cache,
            email_sender: self.email_sender,
            llm_client_factory: self.llm_client_factory,
            http_client_factory: HttpClientFactory::new()
                .map_err(|error| format!("HTTP 客户端工厂初始化失败: {error}"))?,
            settings: self.settings,
            permission_routes,
            jwt_codec: EcJwtCodec::new(jwt_key_pair.encoding_key, jwt_key_pair.decoding_key),
        });

        tracing::info!(target: "app.startup", "AppState 构建完成，开始合并业务路由与 OpenAPI");

        let mut openapi_doc = ApiDoc::openapi();
        let mut business_router = Router::new();

        for (feature_index, feature) in self.features.into_iter().enumerate() {
            tracing::info!(
                target: "app.startup",
                feature_index,
                "开始生成 Feature OpenAPI"
            );

            if let Some(feature_openapi) = feature.openapi() {
                openapi_doc.merge(feature_openapi);
            }

            tracing::info!(
                target: "app.startup",
                feature_index,
                "Feature OpenAPI 生成完成，开始注册路由"
            );

            business_router = feature.register(business_router, app_state.clone());

            tracing::info!(
                target: "app.startup",
                feature_index,
                "Feature 路由注册完成"
            );
        }

        tracing::info!(target: "app.startup", "业务路由注册完成，开始增强 OpenAPI");

        apply_openapi_enhancements(&mut openapi_doc);

        tracing::info!(target: "app.startup", "OpenAPI 增强完成，开始构建宿主路由");

        let mut router_builder = WebApiRouterBuilder::<AppState>::new(business_router, openapi_doc)
            .with_cors_settings(cors_settings);

        if matches!(
            app_state.settings.file.component,
            FileStorageComponent::Local
        ) {
            let upload_root = Path::new(&app_state.settings.file.root_path).join("uploads");
            router_builder = router_builder.with_public_static_dir("/uploads", upload_root);
        }

        if rate_limit_settings.enabled {
            let limiter = Arc::new(RateLimiter::new(
                rate_limit_settings.max_requests,
                rate_limit_settings.window_seconds,
            ));
            tracing::info!(
                target: "app.startup",
                max_requests = rate_limit_settings.max_requests,
                window_seconds = rate_limit_settings.window_seconds,
                "限流中间件已启用"
            );
            router_builder = router_builder.with_rate_limiter(limiter);
        }

        let app = router_builder.build().with_state(app_state.clone());

        tracing::info!(target: "app.startup", "宿主路由构建完成，开始处理启动后任务");

        if should_refresh_identifier_sign {
            RefreshSignTask::new(
                Arc::new(identifier_generator.clone()),
                Arc::clone(&distributed_cache),
            )
            .run()
            .await;
        }

        tracing::info!(target: "app.startup", "宿主构建完成，准备进入运行态");

        Ok(WebApiHost {
            app,
            listen_address: app_state.settings.host.listen_address.clone(),
            _logger_runtime: logger_runtime,
        })
    }
}

impl WebApiHost {
    /// 启动 WebAPI 宿主
    pub async fn run(self) -> Result<(), String> {
        let listener = WebApiStartup::bind_listener(&self.listen_address).await?;
        let bound_address = listener
            .local_addr()
            .map_err(|error| format!("读取监听地址失败: {error}"))?;

        tracing::info!(
            target: "app.startup",
            configured_listen_address = %self.listen_address,
            bound_address = %bound_address,
            "WebAPI 启动完成"
        );

        for swagger_ui_url in build_swagger_ui_urls(bound_address) {
            tracing::info!(
                target: "app.startup",
                swagger_ui_url = %swagger_ui_url,
                "Swagger UI 地址"
            );
        }

        axum::serve(listener, self.app)
            .await
            .map_err(|error| format!("WebAPI 运行失败: {error}"))
    }
}

fn build_swagger_ui_urls(bound_address: SocketAddr) -> Vec<String> {
    match bound_address.ip() {
        IpAddr::V4(ipv4) if ipv4.is_unspecified() => vec![
            format!("http://127.0.0.1:{}/swagger-ui", bound_address.port()),
            format!("http://localhost:{}/swagger-ui", bound_address.port()),
        ],
        IpAddr::V6(ipv6) if ipv6.is_unspecified() => vec![
            format!("http://[::1]:{}/swagger-ui", bound_address.port()),
            format!("http://localhost:{}/swagger-ui", bound_address.port()),
        ],
        IpAddr::V4(ipv4) => vec![format!(
            "http://{}:{}/swagger-ui",
            ipv4,
            bound_address.port()
        )],
        IpAddr::V6(ipv6) => vec![format!(
            "http://[{}]:{}/swagger-ui",
            ipv6,
            bound_address.port()
        )],
    }
}

/// 获取锁组件名称
fn lock_component_name(settings: &Settings) -> &'static str {
    match settings.lock.component {
        crate::host::settings::LockComponent::InMemory => "in_memory",
        crate::host::settings::LockComponent::Redis => "redis",
    }
}

/// 获取缓存组件名称
fn cache_component_name(settings: &Settings) -> &'static str {
    match settings.cache.component {
        CacheComponent::InMemory => "in_memory",
        CacheComponent::Redis => "redis",
    }
}

/// 获取文件存储组件名称
fn file_component_name(settings: &Settings) -> &'static str {
    match settings.file.component {
        FileStorageComponent::Local => "local",
        FileStorageComponent::AliyunOss => "aliyun_oss",
        FileStorageComponent::TencentCos => "tencent_cos",
    }
}
