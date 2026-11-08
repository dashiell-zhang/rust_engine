//! 控制器端点的路径匹配与原始请求地址保护。

use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    extract::{OriginalUri, Request},
    handler::Handler,
    http::{HeaderValue, Method, StatusCode, Uri, Version, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{MethodFilter, MethodRouter as AxumMethodRouter},
};
use tower::ServiceExt;

pub use crate::action::BoundHandler;

type Binding<S> = Box<dyn FnOnce(AxumMethodRouter<S>, bool) -> AxumMethodRouter<S> + Send>;
type Configure<S> = Box<dyn FnOnce(AxumMethodRouter<S>) -> AxumMethodRouter<S> + Send>;

/// 控制器方法及分阶段执行管线。
pub struct MethodRouter<S> {
    action: AxumMethodRouter<S>,
    bindings: Vec<Binding<S>>,
    outer: Vec<Configure<S>>,
    methods: Vec<Method>,
    replay: bool,
}

impl<S: Clone + Send + Sync + 'static> MethodRouter<S> {
    fn on<H, T>(filter: MethodFilter, method: Method, handler: H) -> Self
    where
        H: Clone + Send + Sync + 'static,
        BoundHandler<H, S>: Handler<T, S>,
        T: 'static,
    {
        Self {
            action: axum::routing::on(filter, crate::action::invoke),
            bindings: vec![Box::new(move |action, replay| {
                axum::routing::on(
                    filter,
                    BoundHandler {
                        handler,
                        action,
                        replay,
                    },
                )
            })],
            outer: Vec::new(),
            methods: vec![method],
            replay: false,
        }
    }

    /// 在已有控制器端点上追加 POST。
    pub fn post<H, T>(mut self, handler: H) -> Self
    where
        H: Clone + Send + Sync + 'static,
        BoundHandler<H, S>: Handler<T, S>,
        T: 'static,
    {
        self.action = self.action.post(crate::action::invoke);
        self.bindings.push(Box::new(move |action, replay| {
            axum::routing::post(BoundHandler {
                handler,
                action,
                replay,
            })
        }));
        self.methods.push(Method::POST);
        self
    }

    /// 声明 JSON 正文的参数名，用于正文绑定诊断。
    pub fn json_body(self, parameter: &'static str) -> Self {
        assert!(!parameter.is_empty(), "JSON 正文参数名称不能为空");
        self.map(move |route| route.layer(axum::Extension(crate::json_body::Parameter(parameter))))
    }

    /// 装配绑定之前的认证、权限和正文限制。
    pub fn map(
        mut self,
        configure: impl FnOnce(AxumMethodRouter<S>) -> AxumMethodRouter<S> + Send + 'static,
    ) -> Self {
        self.outer.push(Box::new(configure));
        self
    }

    /// 装配绑定之后的签名、缓存和请求队列等 Action 过滤器。
    pub(crate) fn map_action(
        mut self,
        configure: impl FnOnce(AxumMethodRouter<S>) -> AxumMethodRouter<S>,
    ) -> Self {
        self.action = configure(self.action);
        self.replay = true;
        self
    }

    fn finish(routes: Vec<Self>) -> AxumMethodRouter<S> {
        let mut result = AxumMethodRouter::new();
        let mut methods = Vec::new();
        for route in routes {
            let mut bound = AxumMethodRouter::new();
            for binding in route.bindings {
                bound = bound.merge(binding(route.action.clone(), route.replay));
            }
            for configure in route.outer {
                bound = configure(bound);
            }
            result = result.merge(bound);
            methods.extend(route.methods);
        }
        methods.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
        methods.dedup();
        let allow = HeaderValue::from_str(
            &methods
                .iter()
                .map(Method::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        )
        .expect("已声明的 HTTP 方法");
        let reject = move || {
            let allow = allow.clone();
            async move { (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, allow)]).into_response() }
        };
        result.head(reject.clone()).fallback(reject)
    }
}

macro_rules! controller_method {
    ($name:ident, $method:ident) => {
        #[doc = concat!("声明控制器 ", stringify!($method), " 端点。")]
        pub fn $name<H, T, S>(handler: H) -> MethodRouter<S>
        where
            H: Clone + Send + Sync + 'static,
            BoundHandler<H, S>: Handler<T, S>,
            T: 'static,
            S: Clone + Send + Sync + 'static,
        {
            MethodRouter::on(MethodFilter::$method, Method::$method, handler)
        }
    };
}

controller_method!(get, GET);
controller_method!(post, POST);
controller_method!(delete, DELETE);

/// 注册业务端点时同时收集匹配模板；不依赖 OpenAPI 或权限清单推导路由。
pub struct WebApiRoutes<S> {
    routes: HashMap<String, Vec<MethodRouter<S>>>,
    paths: HashMap<String, String>,
}

impl<S: Clone + Send + Sync + 'static> Default for WebApiRoutes<S> {
    fn default() -> Self {
        Self {
            routes: HashMap::new(),
            paths: HashMap::new(),
        }
    }
}

impl<S: Clone + Send + Sync + 'static> WebApiRoutes<S> {
    /// 注册端点。路由模板的静态部分不区分 ASCII 大小写。
    pub fn route(mut self, path: &str, method_router: MethodRouter<S>) -> Self {
        // 当前控制器使用字面量段及完整的命名参数段；复杂模板不可静默降级。
        for segment in path.split('/') {
            assert!(
                !segment.contains(['{', '}'])
                    || (segment.starts_with('{')
                        && segment.ends_with('}')
                        && !segment.contains('*')
                        && !segment[1..segment.len() - 1].contains(['{', '}'])),
                "不支持的控制器路由模板: {path}"
            );
        }
        let key = path.to_ascii_lowercase();
        if let Some(existing) = self.paths.get(&key) {
            assert_eq!(existing, path, "控制器路由不能只通过大小写区分");
        } else {
            self.paths.insert(key, path.to_owned());
        }
        self.routes
            .entry(path.into())
            .or_default()
            .push(method_router);
        self
    }

    pub(crate) fn into_parts(self) -> (Router<S>, RoutePaths) {
        let mut router = Router::new();
        for (path, route) in self.routes {
            router = router.route(&path, MethodRouter::finish(route));
        }
        let mut paths = RoutePaths::default();
        for (key, path) in self.paths {
            if path.contains('{') {
                paths.parameters.push(path);
            } else {
                paths.literals.insert(key, path);
            }
        }
        // 与端点路由一致：较早的字面量段优先于参数段，不取决于注册顺序。
        paths.parameters.sort_by_key(|path| {
            path.split('/')
                .map(|segment| segment.starts_with('{'))
                .collect::<Vec<_>>()
        });
        (router, paths)
    }
}

#[derive(Default)]
pub(crate) struct RoutePaths {
    literals: HashMap<String, String>,
    parameters: Vec<String>,
}

impl RoutePaths {
    pub(crate) fn add_literal(&mut self, path: &str) {
        self.literals.insert(path.to_ascii_lowercase(), path.into());
    }

    fn canonical_path(&self, path: &str) -> Option<String> {
        // ASP.NET 控制器端点允许一个末尾斜杠；不折叠中间或连续斜杠。
        let path = path.strip_suffix('/').unwrap_or(path);
        if let Some(canonical) = self.literals.get(&path.to_ascii_lowercase()) {
            return Some(canonical.clone());
        }
        let input: Vec<_> = path.split('/').collect();
        for template in &self.parameters {
            let segments: Vec<_> = template.split('/').collect();
            if segments.len() != input.len() {
                continue;
            }
            let mut canonical = Vec::with_capacity(segments.len());
            let mut matched = true;
            for (declared, actual) in segments.iter().zip(&input) {
                if declared.starts_with('{') && !actual.is_empty() {
                    canonical.push(*actual);
                } else if declared.eq_ignore_ascii_case(actual) {
                    canonical.push(*declared);
                } else {
                    matched = false;
                    break;
                }
            }
            if matched {
                return Some(canonical.join("/"));
            }
        }
        None
    }
}

#[derive(Clone)]
struct IncomingUri(Uri);

/// 必须位于内层 Router 之外：Axum 的普通 layer 执行时已经完成路由选择。
pub(crate) fn dispatch(router: Router, paths: RoutePaths) -> Router {
    let paths = Arc::new(paths);
    let router = router.layer(axum::middleware::from_fn(restore_uri));
    Router::new().fallback_service(tower::service_fn(move |mut request: Request| {
        let paths = paths.clone();
        let router = router.clone();
        async move {
            let path = match crate::request_path::RequestPath::parse(request.uri().path()) {
                Ok(path) => path,
                Err(()) => {
                    let mut response = StatusCode::BAD_REQUEST.into_response();
                    // Kestrel 将非法请求路径视为传输层错误，HTTP/1 不再复用连接。
                    if matches!(request.version(), Version::HTTP_10 | Version::HTTP_11) {
                        response
                            .headers_mut()
                            .insert(header::CONNECTION, HeaderValue::from_static("close"));
                    }
                    return Ok(response);
                }
            };
            let canonical = paths.canonical_path(path.value());
            let routing_path = crate::request_path::routing_path(
                canonical.as_deref().unwrap_or_else(|| path.value()),
            );
            request.extensions_mut().insert(path);
            if routing_path != request.uri().path() {
                let original = request.uri().clone();
                let mut parts = original.clone().into_parts();
                let path_and_query = match original.query() {
                    Some(query) => format!("{routing_path}?{query}"),
                    None => routing_path,
                };
                parts.path_and_query = Some(path_and_query.parse().expect("已转义的路由 URI"));
                *request.uri_mut() = Uri::from_parts(parts).expect("已转义的路由 URI 部件");
                request.extensions_mut().insert(IncomingUri(original));
            }
            let mut response = router.oneshot(request).await?;
            // MethodRouter 在 fallback 返回后仍可能补 Allow，预检已由 CORS 完成。
            if response
                .extensions_mut()
                .remove::<crate::cors::CompletedPreflight>()
                .is_some()
            {
                response.headers_mut().remove(header::ALLOW);
            }
            Ok::<_, std::convert::Infallible>(response)
        }
    }))
}

async fn restore_uri(mut request: Request, next: Next) -> Response {
    if let Some(IncomingUri(original)) = request.extensions_mut().remove::<IncomingUri>() {
        *request.uri_mut() = original.clone();
        request.extensions_mut().insert(OriginalUri(original));
    }
    next.run(request).await
}
