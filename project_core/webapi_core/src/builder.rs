use std::{path::PathBuf, sync::Arc, time::Duration};

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method},
    middleware::from_fn_with_state,
    routing::{MethodRouter, get},
};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer, MaxAge};
use tower_http::services::ServeDir;
use url::Url;
use utoipa::openapi::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    auth::{
        JwtDecodingState, PermissionMiddlewareState, PermissionState, RoutePermissionMetadata,
        TokenRenewalState, TokenValidationState, permission_middleware,
    },
    health::{HealthState, healthz},
    host::settings::CorsSettings,
    rate_limit::{RateLimiter, rate_limit_middleware},
    request_log::request_log_middleware,
};

/// WebAPI 路由构建器
pub struct WebApiRouterBuilder<TState> {
    /// 业务路由
    business_router: Router<Arc<TState>>,

    /// OpenAPI 文档
    openapi_doc: OpenApi,

    /// 可选限流器，None 表示不启用限流
    rate_limiter: Option<Arc<RateLimiter>>,

    /// CORS 配置
    cors_settings: CorsSettings,

    /// 公开静态目录挂载配置
    public_static_mounts: Vec<(String, PathBuf)>,
}

impl<TState> WebApiRouterBuilder<TState>
where
    TState: HealthState + Send + Sync + 'static,
{
    /// 创建 WebAPI 路由构建器
    pub fn new(business_router: Router<Arc<TState>>, openapi_doc: OpenApi) -> Self {
        Self {
            business_router,
            openapi_doc,
            rate_limiter: None,
            cors_settings: CorsSettings::default(),
            public_static_mounts: Vec::new(),
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

    /// 挂载公开静态目录
    pub fn with_public_static_dir(
        mut self,
        route_prefix: impl Into<String>,
        directory: impl Into<PathBuf>,
    ) -> Self {
        self.public_static_mounts
            .push((route_prefix.into(), directory.into()));
        self
    }

    /// 构建包含公共路由与中间件的宿主路由
    pub fn build(self) -> Router<Arc<TState>> {
        let mut router = Router::<Arc<TState>>::new()
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", self.openapi_doc))
            .route("/healthz", get(healthz::<TState>))
            .merge(self.business_router)
            .layer(build_cors_layer(&self.cors_settings))
            .layer(axum::middleware::from_fn(request_log_middleware));

        for (route_prefix, directory) in self.public_static_mounts {
            router = router.nest_service(&route_prefix, ServeDir::new(directory));
        }

        if let Some(limiter) = self.rate_limiter {
            router = router.layer(from_fn_with_state(limiter, rate_limit_middleware));
        }

        router
    }
}

fn build_cors_layer(settings: &CorsSettings) -> CorsLayer {
    let allowed_origin_list = settings.allowed_origin_list.clone();

    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _request_parts| {
            is_allowed_cors_origin(origin, &allowed_origin_list)
        }))
        .allow_methods(AllowMethods::list([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
            Method::HEAD,
        ]))
        .allow_headers(AllowHeaders::mirror_request())
        .expose_headers([
            HeaderName::from_static("content-disposition"),
            HeaderName::from_static("content-length"),
            HeaderName::from_static("etag"),
        ])
        .allow_credentials(true)
        .max_age(MaxAge::exact(Duration::from_secs(7200)))
        .allow_private_network(true)
}

fn is_allowed_cors_origin(origin: &HeaderValue, allowed_origin_list: &[String]) -> bool {
    let origin = match origin.to_str() {
        Ok(origin) => origin,
        Err(_) => return false,
    };

    let origin_url = match Url::parse(origin) {
        Ok(origin_url) => origin_url,
        Err(_) => return false,
    };

    let origin_host = match origin_url.host_str() {
        Some(origin_host) => origin_host,
        None => return false,
    };

    let origin_scheme = origin_url.scheme();
    let origin_port = origin_url.port_or_known_default();

    for allowed_host in allowed_origin_list {
        if allowed_host == "*" {
            return true;
        }

        if is_cors_host_match(origin_scheme, origin_host, origin_port, allowed_host) {
            return true;
        }
    }

    false
}

fn is_cors_host_match(
    origin_scheme: &str,
    origin_host: &str,
    origin_port: Option<u16>,
    allowed_host: &str,
) -> bool {
    if allowed_host.trim().is_empty() {
        return false;
    }

    let mut allowed_host_pattern = allowed_host.trim();
    let mut allowed_scheme = None;
    let mut allowed_port = None;

    if allowed_host_pattern.len() >= "https://".len()
        && allowed_host_pattern[.."https://".len()].eq_ignore_ascii_case("https://")
    {
        allowed_scheme = Some("https");
        allowed_host_pattern = &allowed_host_pattern["https://".len()..];
    } else if allowed_host_pattern.len() >= "http://".len()
        && allowed_host_pattern[.."http://".len()].eq_ignore_ascii_case("http://")
    {
        allowed_scheme = Some("http");
        allowed_host_pattern = &allowed_host_pattern["http://".len()..];
    }

    if let Some(allowed_scheme) = allowed_scheme
        && !origin_scheme.eq_ignore_ascii_case(allowed_scheme)
    {
        return false;
    }

    if let Some((host_pattern, port_text)) = allowed_host_pattern.rsplit_once(':')
        && let Ok(parsed_port) = port_text.parse::<u16>()
    {
        allowed_host_pattern = host_pattern;
        allowed_port = Some(parsed_port);
    }

    if let Some(allowed_port) = allowed_port
        && origin_port != Some(allowed_port)
    {
        return false;
    }

    if let Some(domain_suffix) = allowed_host_pattern.strip_prefix("*.") {
        let domain_suffix = format!(".{domain_suffix}");
        return origin_host.ends_with(&domain_suffix);
    }

    origin_host.eq_ignore_ascii_case(allowed_host_pattern)
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
    TState: JwtDecodingState
        + TokenValidationState
        + PermissionState
        + TokenRenewalState
        + Send
        + Sync
        + 'static,
{
    let middleware_state = PermissionMiddlewareState {
        state,
        route_permission: RoutePermissionMetadata::new(module, route_path, allow_unbound),
    };

    route.layer(from_fn_with_state(
        middleware_state,
        permission_middleware::<TState>,
    ))
}
