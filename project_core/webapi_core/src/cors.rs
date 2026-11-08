use crate::host::settings::CorsSettings;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use url::Url;

/// 标记已由 CORS 短路完成的预检，供最外层移除框架后加的 Allow。
#[derive(Clone)]
pub(crate) struct CompletedPreflight;

/// Swagger 的 GET 在源管线中提前完成；其他方法仍会经过后续 CORS。
pub(crate) async fn swagger_cors_middleware(
    state: State<Arc<CorsSettings>>,
    request: Request,
    next: Next,
) -> Response {
    if request.method() == Method::GET {
        next.run(request).await
    } else {
        cors_middleware(state, request, next).await
    }
}

/// 源宿主使用自定义来源白名单、任意请求方法/头、凭据和两小时预检缓存。
pub(crate) async fn cors_middleware(
    State(settings): State<Arc<CorsSettings>>,
    request: Request,
    next: Next,
) -> Response {
    if !request.headers().contains_key(header::ORIGIN) {
        return next.run(request).await;
    }
    let origin = joined_header(request.headers(), header::ORIGIN);
    let allowed = origin
        .as_deref()
        .is_some_and(|value| is_allowed_cors_origin(value, &settings.allowed_origin_list));
    let preflight = request.method() == Method::OPTIONS
        && request
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
    let methods = preflight
        .then(|| joined_header(request.headers(), header::ACCESS_CONTROL_REQUEST_METHOD))
        .flatten();
    let headers = preflight
        .then(|| joined_header(request.headers(), header::ACCESS_CONTROL_REQUEST_HEADERS))
        .flatten();
    let mut response = if preflight {
        let mut response = StatusCode::NO_CONTENT.into_response();
        response.extensions_mut().insert(CompletedPreflight);
        response
    } else {
        next.run(request).await
    };
    if !allowed {
        return response;
    }
    if let Some(origin) = origin.and_then(|value| HeaderValue::from_str(&value).ok()) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("Origin"));
    if preflight {
        if let Some(methods) = methods.and_then(|value| HeaderValue::from_str(&value).ok()) {
            response
                .headers_mut()
                .insert(header::ACCESS_CONTROL_ALLOW_METHODS, methods);
        }
        if let Some(headers) = headers
            .filter(|value| !value.is_empty())
            .and_then(|value| HeaderValue::from_str(&value).ok())
        {
            response
                .headers_mut()
                .insert(header::ACCESS_CONTROL_ALLOW_HEADERS, headers);
        }
        response.headers_mut().insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("7200"),
        );
    }
    // 不全局声明 Expose-Headers；续签等功能在产生该头时自行声明。
    response
}

fn joined_header(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    if !headers.contains_key(&name) {
        return None;
    }
    headers
        .get_all(name)
        .iter()
        .map(|value| value.to_str().ok())
        .collect::<Option<Vec<_>>>()
        .map(|values| values.join(","))
}

fn is_allowed_cors_origin(origin: &str, allowed_origin_list: &[String]) -> bool {
    if origin.trim().is_empty() {
        return false;
    }
    let Ok(origin_url) = Url::parse(origin) else {
        return false;
    };
    let host = origin_url.host_str().unwrap_or_default();
    let port = origin_url
        .port_or_known_default()
        .map(i32::from)
        .unwrap_or(-1);
    allowed_origin_list.iter().any(|pattern| {
        pattern == "*" || is_cors_host_match(origin_url.scheme(), host, port, pattern)
    })
}

fn is_cors_host_match(
    origin_scheme: &str,
    origin_host: &str,
    origin_port: i32,
    allowed_host: &str,
) -> bool {
    let mut pattern = allowed_host.trim();
    if pattern.is_empty() {
        return false;
    }
    for prefix in ["https://", "http://"] {
        if pattern
            .get(..prefix.len())
            .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
        {
            if !origin_scheme.eq_ignore_ascii_case(&prefix[..prefix.len() - 3]) {
                return false;
            }
            pattern = &pattern[prefix.len()..];
            break;
        }
    }
    if let Some((host, port)) = pattern.rsplit_once(':')
        && !host.is_empty()
        && let Ok(port) = port.trim().parse::<i32>()
    {
        if port != origin_port {
            return false;
        }
        pattern = host;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        origin_host
            .to_ascii_lowercase()
            .ends_with(&format!(".{}", suffix.to_ascii_lowercase()))
    } else {
        origin_host.eq_ignore_ascii_case(pattern)
    }
}
