use std::sync::{Arc, OnceLock};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::Response,
};

use crate::{error::UnhandledError, request_context::RequestContext};

/// 显式端口、宿主环境配置优先，其次采用实际 HTTPS 监听端口。
#[derive(Default)]
pub(crate) struct HttpsPolicy {
    configured_port: Option<u16>,
    resolved_port: OnceLock<Option<u16>>,
    listener_port: Arc<OnceLock<u16>>,
}

impl HttpsPolicy {
    pub(crate) fn new(configured_port: Option<u16>, listener_port: Arc<OnceLock<u16>>) -> Self {
        Self {
            configured_port,
            resolved_port: OnceLock::new(),
            listener_port,
        }
    }

    fn port(&self) -> Option<u16> {
        *self.resolved_port.get_or_init(|| {
            let port = self
                .configured_port
                .or_else(|| {
                    [
                        "HTTPS_PORT",
                        "DOTNET_HTTPS_PORT",
                        "ASPNETCORE_HTTPS_PORT",
                        "ANCM_HTTPS_PORT",
                        "DOTNET_ANCM_HTTPS_PORT",
                        "ASPNETCORE_ANCM_HTTPS_PORT",
                    ]
                    .into_iter()
                    .find_map(|key| std::env::var(key).ok()?.trim().parse().ok())
                })
                .or_else(|| self.listener_port.get().copied());
            if port.is_none() {
                tracing::warn!(target: "app.http", "无法确定 HTTPS 重定向端口，继续处理 HTTP 请求");
            }
            port
        })
    }
}

pub(crate) async fn https_policy(
    State(policy): State<Arc<HttpsPolicy>>,
    context: RequestContext,
    request: Request,
    next: Next,
) -> Response {
    if !context.scheme().eq_ignore_ascii_case("https") {
        if let Some(port) = policy.port() {
            let host = context.host();
            let authority = if port == 443 {
                host.to_owned()
            } else {
                format!("{host}:{port}")
            };
            let location = format!(
                "https://{authority}{}",
                crate::request_path::path_and_query(request.uri(), request.extensions())
            );
            let Ok(location) = HeaderValue::from_str(&location) else {
                return crate::error::internal_error_response("无法构造 HTTPS 重定向地址");
            };
            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::TEMPORARY_REDIRECT;
            response.headers_mut().insert(header::LOCATION, location);
            return response;
        }
        return next.run(request).await;
    }

    let excluded = ["localhost", "127.0.0.1", "[::1]"]
        .iter()
        .any(|host| context.host().eq_ignore_ascii_case(host));
    let mut response = next.run(request).await;
    // 全局异常响应会清空先前的响应头；普通业务错误继续保留 HSTS。
    if !excluded && response.extensions().get::<UnhandledError>().is_none() {
        response
            .headers_mut()
            .entry(header::STRICT_TRANSPORT_SECURITY)
            .or_insert(HeaderValue::from_static("max-age=31536000"));
    }
    response
}

/// 开发文档 GET 先于公共 HTTPS 管线，其他方法仍进入公共管线。
pub(crate) async fn swagger_https_policy(
    state: State<Arc<HttpsPolicy>>,
    context: RequestContext,
    request: Request,
    next: Next,
) -> Response {
    if request.method() == Method::GET {
        next.run(request).await
    } else {
        https_policy(state, context, request, next).await
    }
}
