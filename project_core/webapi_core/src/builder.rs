use crate::routes::MethodRouter;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use axum::{Router, middleware::from_fn_with_state, routing::any};
use utoipa::openapi::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    auth::{
        JwtDecodingState, PermissionMiddlewareState, PermissionState, RoutePermissionMetadata,
        TokenRenewalState, permission_middleware,
    },
    health::{HealthState, healthz},
    host::settings::CorsSettings,
    rate_limit::{RateLimiter, rate_limit_middleware},
    request_log::request_log_middleware,
};

/// WebAPI 路由构建器
pub struct WebApiRouterBuilder<TState> {
    /// 业务路由
    business_router: crate::routes::WebApiRoutes<Arc<TState>>,

    /// OpenAPI 文档
    openapi_doc: OpenApi,

    /// 可选限流器，None 表示不启用限流
    rate_limiter: Option<Arc<RateLimiter>>,

    /// CORS 配置
    cors_settings: CorsSettings,

    /// 可选公开 Web 根目录；业务端点优先匹配。
    public_web_root: Option<PathBuf>,

    /// 外部 HTTPS 重定向端口。
    https_port: Option<u16>,
    https_listener_port: Arc<OnceLock<u16>>,
    request_tasks: Option<crate::request_cancellation::RequestTasks>,
}

impl<TState> WebApiRouterBuilder<TState>
where
    TState: HealthState + Send + Sync + 'static,
{
    /// 创建 WebAPI 路由构建器
    pub fn new(
        business_router: crate::routes::WebApiRoutes<Arc<TState>>,
        openapi_doc: OpenApi,
    ) -> Self {
        Self {
            business_router,
            openapi_doc,
            rate_limiter: None,
            cors_settings: CorsSettings::default(),
            public_web_root: None,
            https_port: None,
            https_listener_port: Arc::new(OnceLock::new()),
            request_tasks: None,
        }
    }

    /// 挂载限流器
    pub fn with_rate_limiter(mut self, limiter: Arc<RateLimiter>) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    /// 挂载 CORS 配置
    pub fn with_cors_settings(mut self, cors_settings: CorsSettings) -> Self {
        self.cors_settings = cors_settings;
        self
    }

    /// 挂载公开 Web 根目录，不自动提供目录首页。
    pub fn with_public_web_root(mut self, directory: impl Into<PathBuf>) -> Self {
        self.public_web_root = Some(directory.into());
        self
    }

    /// 配置当前宿主对外 HTTPS 重定向端口。
    pub fn with_https_port(mut self, port: Option<u16>) -> Self {
        self.https_port = port;
        self
    }

    pub(crate) fn with_https_listener_port(mut self, port: Arc<OnceLock<u16>>) -> Self {
        self.https_listener_port = port;
        self
    }

    pub(crate) fn with_request_tasks(
        mut self,
        tasks: crate::request_cancellation::RequestTasks,
    ) -> Self {
        self.request_tasks = Some(tasks);
        self
    }

    /// 构建包含公共路由与中间件的宿主路由
    pub fn build(self, state: Arc<TState>) -> Router {
        let cors_settings = Arc::new(self.cors_settings);
        let https_policy = Arc::new(crate::https_policy::HttpsPolicy::new(
            self.https_port,
            self.https_listener_port,
        ));
        let (business_router, mut paths) = self.business_router.into_parts();
        paths.add_literal("/healthz");
        let mut router = Router::<Arc<TState>>::new()
            .route("/healthz", any(healthz::<TState>))
            .merge(business_router.layer(axum::middleware::from_fn(
                crate::object_response::format_object_response,
            )));
        if let Some(directory) = self.public_web_root {
            router = router.fallback_service(tower::service_fn(move |request| {
                let directory = directory.clone();
                async move {
                    Ok::<_, std::convert::Infallible>(
                        crate::static_files::serve(directory, request).await,
                    )
                }
            }));
        }
        router = router.layer(from_fn_with_state(
            cors_settings.clone(),
            crate::cors::cors_middleware,
        ));
        if let Some(limiter) = self.rate_limiter {
            router = router.layer(from_fn_with_state(limiter, rate_limit_middleware));
        }
        router = router
            .layer(from_fn_with_state(
                https_policy.clone(),
                crate::https_policy::https_policy,
            ))
            .layer(axum::middleware::from_fn(
                crate::error::exception_middleware,
            ));
        // 源宿主 Swagger 在 CORS 等业务中间件之前直接响应。
        if crate::environment::is_development() {
            let swagger = Router::new()
                .merge(SwaggerUi::new("/swagger").url("/swagger/v1/swagger.json", self.openapi_doc))
                .layer(from_fn_with_state(
                    cors_settings,
                    crate::cors::swagger_cors_middleware,
                ))
                .layer(from_fn_with_state(
                    https_policy,
                    crate::https_policy::swagger_https_policy,
                ));
            router = router.merge(swagger);
        } else {
            router = router.layer(axum::middleware::from_fn(
                crate::response_compression::response_compression,
            ));
        }
        router = router
            .layer(axum::middleware::from_fn(request_log_middleware))
            .layer(axum::middleware::from_fn(
                crate::request_context::forwarded_headers,
            ))
            .layer(from_fn_with_state(
                self.request_tasks,
                crate::request_cancellation::request_cancellation,
            ));

        crate::routes::dispatch(router.with_state(state), paths)
    }
}

/// 为单个路由挂载权限元数据与权限守卫
pub fn protect_method_router<TState>(
    route: MethodRouter<std::sync::Arc<TState>>,
    state: std::sync::Arc<TState>,
    module: impl Into<String>,
    route_path: impl Into<String>,
    allow_unbound: bool,
) -> MethodRouter<std::sync::Arc<TState>>
where
    TState: JwtDecodingState + PermissionState + TokenRenewalState + Send + Sync + 'static,
{
    let middleware_state = PermissionMiddlewareState {
        state,
        route_permission: RoutePermissionMetadata::new(module, route_path, allow_unbound),
    };

    route.map(move |route| {
        route.route_layer(from_fn_with_state(
            middleware_state,
            permission_middleware::<TState>,
        ))
    })
}
