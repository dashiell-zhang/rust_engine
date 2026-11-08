//! 同一端点对象装配运行路由、权限同步及文档；特性只枚举一次处理器。
use crate::{
    host::{features::WebApiRoutePermission, state::AppState},
    routes::{MethodRouter, WebApiRoutes},
};
use axum::http::Method;
use std::sync::Arc;
use utoipa::openapi::OpenApi;

type Route = MethodRouter<Arc<AppState>>;
type Configure = Box<dyn FnOnce(Route, Arc<AppState>) -> Route + Send>;

#[derive(Clone, Copy)]
pub enum EndpointPolicy {
    Anonymous,
    Signed,
    Authorized,
    AuthorizedSigned,
}

pub struct Endpoint {
    path: &'static str,
    methods: &'static [Method],
    policy: EndpointPolicy,
    permission: Option<&'static str>,
    route: fn() -> Route,
    document: fn() -> Option<OpenApi>,
    action_configuration: Vec<Configure>,
    outer_configuration: Vec<Configure>,
}

impl Endpoint {
    pub fn path(&self) -> &'static str {
        self.path
    }
    pub fn methods(&self) -> &'static [Method] {
        self.methods
    }
    /// 由 webapi_endpoint 属性生成。普通业务代码通过 endpoint!(处理器路径) 获取。
    #[doc(hidden)]
    pub fn new(
        path: &'static str,
        methods: &'static [Method],
        policy: EndpointPolicy,
        permission: Option<&'static str>,
        route: fn() -> Route,
        document: fn() -> Option<OpenApi>,
    ) -> Self {
        Self {
            path,
            methods,
            policy,
            permission,
            route,
            document,
            action_configuration: Vec::new(),
            outer_configuration: Vec::new(),
        }
    }

    /// 缓存、请求队列等配置先装配，认证/续签/权限仍由外层守卫优先执行。
    pub fn configure_action(
        mut self,
        configure: impl FnOnce(Route, Arc<AppState>) -> Route + Send + 'static,
    ) -> Self {
        self.action_configuration.push(Box::new(configure));
        self
    }

    /// 保留正文限制、multipart 验证等外层配置的原有顺序。
    pub fn configure(
        mut self,
        configure: impl FnOnce(Route, Arc<AppState>) -> Route + Send + 'static,
    ) -> Self {
        self.outer_configuration.push(Box::new(configure));
        self
    }

    pub fn permission(&self) -> Option<WebApiRoutePermission> {
        self.permission.map(|remark| WebApiRoutePermission {
            route: self.path,
            remark,
        })
    }

    pub fn document(&self) -> Option<OpenApi> {
        (self.document)()
    }

    pub fn register(
        self,
        router: WebApiRoutes<Arc<AppState>>,
        state: Arc<AppState>,
    ) -> WebApiRoutes<Arc<AppState>> {
        let mut route = (self.route)();
        for configure in self.action_configuration {
            route = configure(route, state.clone());
        }
        if matches!(
            self.policy,
            EndpointPolicy::Signed | EndpointPolicy::AuthorizedSigned
        ) {
            route = crate::sign_verify::sign_verify_method_router(route, state.clone());
        }
        if matches!(
            self.policy,
            EndpointPolicy::Authorized | EndpointPolicy::AuthorizedSigned
        ) {
            route = crate::builder::protect_method_router(
                route,
                state.clone(),
                state.settings.permission_module.clone(),
                self.path,
                false,
            );
        }
        for configure in self.outer_configuration {
            route = configure(route, state.clone());
        }
        router.route(self.path, route)
    }
}
