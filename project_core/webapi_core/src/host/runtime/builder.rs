use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, OnceLock},
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
use axum::Router;
use http_client::HttpClientFactory;
use identifier_generator::IdentifierGenerator;
use jwt_auth::EcJwtCodec;
use logger::LoggerRuntime;
use utoipa::OpenApi;

/// WebAPI 宿主构建器
pub struct WebApiHostBuilder {
    /// 宿主配置
    settings: Settings,

    /// 分布式锁构建方法
    build_distributed_lock: BuildWebApiLock,

    /// 缓存构建方法
    build_cache: BuildWebApiCache,

    /// 日志运行时构建方法
    build_logger_runtime: BuildWebApiLoggerRuntime,
    build_event_transport: Option<crate::host::BuildWebApiEventTransport>,

    build_resources: Option<crate::host::BuildWebApiResources>,
    application: Option<Arc<dyn crate::host::application::WebApiApplication>>,

    /// 已注册特性
    features: Vec<Box<dyn WebApiFeature>>,
    background_services: common::background_service::BackgroundServiceRegistry,
    check_log_read_health: bool,
    serve_web_root: bool,
    configure_http_clients: Option<crate::host::ConfigureWebApiHttpClients>,
}

/// WebAPI 宿主实例
pub struct WebApiHost {
    /// 最终路由
    app: Router,

    /// 监听地址
    host_settings: crate::startup::HostSettings,
    https_listener_port: Arc<OnceLock<u16>>,

    /// 日志运行时守卫
    _logger_runtime: LoggerRuntime,
    event_bus: event_bus::PreparedEventBus,
    event_listeners: event_bus::EventListenerRegistry,
    background_services: common::background_service::BackgroundServiceRegistry,
    request_runtime: crate::request_cancellation::RequestRuntime,
}

impl WebApiHostBuilder {
    /// 注册不依赖 Feature 上下文的常驻服务；需要 AppState 时使用 Feature 注册钩子。
    pub fn with_background_service(
        mut self,
        name: &'static str,
        service: impl common::background_service::BackgroundService,
    ) -> Result<Self, String> {
        self.background_services.register(name, service)?;
        Ok(self)
    }

    /// 创建 WebAPI 宿主构建器
    pub fn new(
        settings: Settings,
        build_distributed_lock: BuildWebApiLock,
        build_cache: BuildWebApiCache,
        build_logger_runtime: BuildWebApiLoggerRuntime,
    ) -> Self {
        Self {
            settings,
            build_distributed_lock,
            build_cache,
            build_logger_runtime,
            features: Vec::new(),
            background_services: common::background_service::BackgroundServiceRegistry::default(),
            build_event_transport: None,
            application: None,
            build_resources: None,
            check_log_read_health: false,
            serve_web_root: false,
            configure_http_clients: None,
        }
    }

    pub fn with_resources_factory(mut self, factory: crate::host::BuildWebApiResources) -> Self {
        self.build_resources = Some(factory);
        self
    }

    /// 由宿主档案决定是否注册日志读库健康检查。
    pub fn with_log_read_health(mut self, enabled: bool) -> Self {
        self.check_log_read_health = enabled;
        self
    }

    /// 管理端把文件根目录公开为静态 Web 根目录。
    pub fn with_web_root(mut self, enabled: bool) -> Self {
        self.serve_web_root = enabled;
        self
    }

    /// 运行入口指定事件后端，禁止公共宿主自行选择实现。
    pub fn with_event_transport_factory(
        mut self,
        factory: crate::host::BuildWebApiEventTransport,
    ) -> Self {
        self.build_event_transport = Some(factory);
        self
    }

    pub fn with_application(
        mut self,
        application: Arc<dyn crate::host::application::WebApiApplication>,
    ) -> Self {
        self.application = Some(application);
        self
    }

    pub fn with_http_client_configuration(
        mut self,
        configure: Option<crate::host::ConfigureWebApiHttpClients>,
    ) -> Self {
        self.configure_http_clients = configure;
        self
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
        let application = self.application.ok_or("WebAPI 宿主未注册业务策略")?;
        let jwt_key_pair = WebApiStartup::parse_ec_jwt_key_pair(
            &self.settings.jwt.private_key,
            &self.settings.jwt.public_key,
        )?;

        let distributed_lock = (self.build_distributed_lock)(&self.settings).await?;
        let distributed_cache = (self.build_cache)(&self.settings).await?;
        let mut http_client_factory = HttpClientFactory::new()
            .map_err(|error| format!("HTTP 客户端工厂初始化失败: {error}"))?;
        if let Some(configure) = self.configure_http_clients {
            configure(
                &self.settings,
                distributed_cache.clone(),
                &mut http_client_factory,
            )?;
        }
        let http_client_factory = Arc::new(http_client_factory);
        let identifier_generator = IdentifierGenerator::from_settings(
            &self.settings.identifier_generator,
            &self.settings.cache.connection,
        )
        .await
        .map_err(|error| format!("ID 服务初始化失败: {error}"))?;
        let cache = Arc::clone(&distributed_cache);

        let resources = match self.build_resources {
            Some(build) => {
                build(
                    &self.settings,
                    self.check_log_read_health,
                    http_client_factory.clone(),
                )
                .await?
            }
            None => crate::host::resources::WebApiResources::default(),
        };

        application.validate_resources(&resources)?;
        for feature in &self.features {
            feature.validate_resources(&resources)?;
        }

        let logger_runtime = (self.build_logger_runtime)(
            &self.settings,
            &WebApiLoggerBuilderContext {
                resources: &resources,
                identifier_generator: identifier_generator.clone(),
            },
        )
        .await?;

        let build_event_transport = self.build_event_transport.ok_or("宿主未注册事件后端")?;
        let transport = build_event_transport(&self.settings, &identifier_generator)?;
        let event_publisher = event_bus::EventPublisher::new(transport.clone());
        let startup_stopping = event_bus::CancellationToken::new();
        let prepare = event_bus::PreparedEventBus::prepare(
            transport,
            self.settings.event_bus.clone(),
            &startup_stopping,
        );
        tokio::pin!(prepare);
        let event_bus = tokio::select! {
            result = &mut prepare => result?,
            signal = common::system_helper::wait_for_shutdown() => {
                startup_stopping.cancel();
                // 让适配器完成服务器端取消，仍遵守宿主三十秒停机上限。
                let _ = tokio::time::timeout(std::time::Duration::from_secs(30), &mut prepare).await;
                signal?;
                return Err("事件总线启动已取消".to_string());
            },
        };
        let mut event_listeners = event_bus::EventListenerRegistry::default();

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

        // 提前取出限流配置，避免 settings move 进 AppState 后无法访问
        let rate_limit_settings = self.settings.rate_limit.clone();
        let cors_settings = self.settings.cors.clone();

        tracing::info!(target: "app.startup", "开始构建 WebAPI 共享状态");

        let app_state = Arc::new(AppState {
            application,
            event_publisher,
            resources,
            id_generator,
            distributed_lock,
            cache,
            http_client_factory,
            settings: self.settings,
            permission_routes,
            jwt_codec: EcJwtCodec::new(jwt_key_pair.encoding_key, jwt_key_pair.decoding_key),
        });

        tracing::info!(target: "app.startup", "AppState 构建完成，开始合并业务路由与 OpenAPI");

        let mut openapi_doc = ApiDoc::openapi();
        let mut business_router = crate::routes::WebApiRoutes::default();

        let mut background_services = self.background_services;
        let health_state = app_state.clone();
        background_services.register("webapi.health", move |stopping| async move {
            crate::health::publish_health(health_state, stopping).await;
            Ok(())
        })?;
        for (feature_index, feature) in self.features.into_iter().enumerate() {
            feature.register_background_services(&mut background_services, app_state.clone())?;
            feature.register_event_listeners(&mut event_listeners, app_state.clone())?;
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

        let https_listener_port = Arc::new(OnceLock::new());
        let request_runtime = crate::request_cancellation::RequestRuntime::new();
        let mut router_builder = WebApiRouterBuilder::<AppState>::new(business_router, openapi_doc)
            .with_request_tasks(request_runtime.tasks())
            .with_cors_settings(cors_settings)
            .with_https_port(app_state.settings.host.https_port)
            .with_https_listener_port(https_listener_port.clone());

        if self.serve_web_root {
            router_builder =
                router_builder.with_public_web_root(&app_state.settings.file.root_path);
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

        let app = router_builder.build(app_state.clone());

        tracing::info!(target: "app.startup", "宿主路由构建完成，开始处理启动后任务");

        tracing::info!(target: "app.startup", "宿主构建完成，准备进入运行态");

        Ok(WebApiHost {
            app,
            host_settings: app_state.settings.host.clone(),
            https_listener_port,
            _logger_runtime: logger_runtime,
            event_bus,
            event_listeners,
            background_services,
            request_runtime,
        })
    }
}

impl WebApiHost {
    /// 启动 WebAPI 宿主
    pub async fn run(mut self) -> Result<(), String> {
        let bound = async {
            let listener = crate::listener::HostListener::bind(&self.host_settings).await?;
            let addresses = listener
                .addresses()
                .map_err(|error| format!("读取监听地址失败: {error}"))?;
            Ok::<_, String>((listener, addresses))
        }
        .await;
        let (listener, bound_addresses) = match bound {
            Ok(bound) => bound,
            Err(error) => {
                logger::critical!(target: "app.startup", %error, "WebAPI 启动失败");
                self._logger_runtime
                    .shutdown(std::time::Duration::from_secs(30))
                    .await;
                return Err(error);
            }
        };

        for connection in bound_addresses {
            if connection.tls {
                let _ = self.https_listener_port.set(connection.address.port());
            }
            let scheme = if connection.tls { "https" } else { "http" };
            tracing::info!(
                target: "app.startup",
                bound_address = %connection.address, scheme,
                "WebAPI 启动完成"
            );
            if crate::environment::is_development() {
                for swagger_ui_url in build_swagger_ui_urls(connection.address, scheme) {
                    tracing::info!(target: "app.startup", %swagger_ui_url, "Swagger UI 地址");
                }
            }
        }

        let mut events = self.event_bus.start(self.event_listeners);
        let shutdown = event_bus::CancellationToken::new();
        let mut background = self.background_services.start(&shutdown);
        let server_shutdown = shutdown.clone();
        let server = axum::serve(
            listener,
            self.app
                .into_make_service_with_connect_info::<crate::listener::ConnectionInfo>(),
        )
        .with_graceful_shutdown(async move { server_shutdown.cancelled().await });
        let server = std::future::IntoFuture::into_future(server);
        tokio::pin!(server);
        let (mut result, drain_server) = tokio::select! {
            result = &mut server => (result.map_err(|error| format!("WebAPI 运行失败: {error}")), false),
            failure = events.wait_failure() => (failure, true),
            failure = background.wait_failure() => (failure, true),
            signal = common::system_helper::wait_for_shutdown() => (signal, true),
        };
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        events.cancel();
        shutdown.cancel();
        if drain_server {
            let drain = tokio::time::timeout_at(deadline, &mut server)
                .await
                .map_err(|_| "WebAPI 停机等待超时".to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            result = result.and(drain);
        }
        if !self.request_runtime.shutdown(deadline).await {
            tracing::warn!(target: "app.shutdown", "请求清理期限耗尽");
            result = result.and(Err("请求停机等待超时".into()));
        }
        result = result.and(background.shutdown(deadline).await);
        drop(background);
        let _ = tokio::time::timeout_at(deadline, events.shutdown()).await;
        drop(events);
        self._logger_runtime
            .shutdown(deadline.saturating_duration_since(tokio::time::Instant::now()))
            .await;
        result
    }
}

fn build_swagger_ui_urls(bound_address: SocketAddr, scheme: &str) -> Vec<String> {
    match bound_address.ip() {
        IpAddr::V4(ipv4) if ipv4.is_unspecified() => vec![
            format!("{scheme}://127.0.0.1:{}/swagger", bound_address.port()),
            format!("{scheme}://localhost:{}/swagger", bound_address.port()),
        ],
        IpAddr::V6(ipv6) if ipv6.is_unspecified() => vec![
            format!("{scheme}://[::1]:{}/swagger", bound_address.port()),
            format!("{scheme}://localhost:{}/swagger", bound_address.port()),
        ],
        IpAddr::V4(ipv4) => vec![format!(
            "{scheme}://{}:{}/swagger",
            ipv4,
            bound_address.port()
        )],
        IpAddr::V6(ipv6) => vec![format!(
            "{scheme}://[{}]:{}/swagger",
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
