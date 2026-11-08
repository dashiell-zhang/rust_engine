use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts, Request},
    http::{Extensions, HeaderMap, HeaderValue, Uri, header, request::Parts},
    middleware::Next,
    response::Response,
};
use common::app_error::AppError;

/// 经过共享代理中间件处理的请求地址；业务使用此上下文生成外部地址。
#[derive(Clone, Debug)]
pub struct RequestContext {
    scheme: String,
    authority: String,
    remote_addr: Option<SocketAddr>,
    client_ip: Option<String>,
}

impl RequestContext {
    /// 当前请求协议，包含受信任代理提供的协议。
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// 当前请求 Host，包含显式端口，不接受 X-Forwarded-Host 覆盖。
    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// 当前请求主机名；IPv6 保留方括号。
    pub fn host(&self) -> &str {
        if self.authority.starts_with('[') {
            return self
                .authority
                .find(']')
                .map_or(&self.authority, |end| &self.authority[..=end]);
        }
        self.authority
            .split_once(':')
            .map_or(&self.authority, |(host, _)| host)
    }

    /// 当前请求的基础 URL，不包含路径。
    pub fn base_url(&self) -> String {
        format!("{}://{}", self.scheme, self.authority)
    }

    /// 当前请求完整 URL。
    pub fn url(&self, uri: &Uri) -> String {
        format!(
            "{}{}",
            self.base_url(),
            crate::request_path::RequestPath::parse(uri.path())
                .map(|path| path.path_and_query(uri))
                .unwrap_or_else(|()| uri
                    .path_and_query()
                    .map_or("/", |value| value.as_str())
                    .to_owned())
        )
    }

    /// 经代理处理后的远端地址。
    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr
    }

    /// 与源 GetRemoteIP 一致，先读取保留的转发头，再回退连接地址。
    pub fn client_ip(&self) -> Result<&str, AppError> {
        self.client_ip
            .as_deref()
            .ok_or_else(|| AppError::internal("RemoteIpAddress 为 null 无法处理"))
    }

    fn from_request(uri: &Uri, headers: &HeaderMap, extensions: &Extensions) -> Self {
        let transport = extensions.get::<ConnectInfo<crate::listener::ConnectionInfo>>();
        let remote_addr = transport.map(|info| info.0.address).or_else(|| {
            extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|info| info.0)
        });
        Self {
            // 以实际连接判断 TLS；absolute-form 请求行不能改写传输协议。
            scheme: if transport.is_some_and(|info| info.0.tls) {
                "https"
            } else {
                "http"
            }
            .to_owned(),
            authority: uri
                .authority()
                .map(|value| value.as_str())
                .or_else(|| {
                    headers
                        .get(header::HOST)
                        .and_then(|value| value.to_str().ok())
                })
                .unwrap_or_default()
                .to_owned(),
            remote_addr,
            client_ip: client_ip(headers, remote_addr),
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for RequestContext {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<Self>()
            .cloned()
            .unwrap_or_else(|| Self::from_request(&parts.uri, &parts.headers, &parts.extensions)))
    }
}

/// NetEngine 只启用 For/Proto，默认从右向左消费一跳。
pub(crate) async fn forwarded_headers(mut request: Request, next: Next) -> Response {
    let mut context =
        RequestContext::from_request(request.uri(), request.headers(), request.extensions());
    let trusted = context.remote_addr.is_none_or(|remote| match remote.ip() {
        IpAddr::V4(_) => true,
        IpAddr::V6(ip) => ip.is_loopback() || ip.to_ipv4_mapped().is_some(),
    });
    if trusted {
        let forwarded_for = comma_values(request.headers(), "x-forwarded-for");
        let forwarded_proto = comma_values(request.headers(), "x-forwarded-proto");
        let ip_text = forwarded_for.last().map(String::as_str).unwrap_or_default();
        let forwarded_addr = parse_endpoint(ip_text);
        // 无法解析的最右侧 IP 会终止本跳，连同本跳协议都不应用。
        if ip_text.is_empty() || forwarded_addr.is_some() {
            let scheme = forwarded_proto.last().filter(|scheme| {
                !scheme.is_empty()
                    && scheme
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"+-.".contains(&byte))
            });
            if forwarded_addr.is_some() || scheme.is_some() {
                if let Some(remote) = context.remote_addr {
                    request.headers_mut().insert(
                        "x-original-for",
                        HeaderValue::from_str(&remote.to_string()).expect("socket address header"),
                    );
                }
                if let Some(remote) = forwarded_addr.or(context.remote_addr) {
                    context.remote_addr = Some(remote);
                    request.extensions_mut().insert(ConnectInfo(remote));
                    truncate_header(request.headers_mut(), "x-forwarded-for", &forwarded_for);
                }
                if let Some(scheme) = scheme {
                    if let Ok(value) = HeaderValue::from_str(&context.scheme) {
                        request.headers_mut().insert("x-original-proto", value);
                    }
                    context.scheme = scheme.clone();
                    truncate_header(request.headers_mut(), "x-forwarded-proto", &forwarded_proto);
                }
            }
        }
    }
    context.client_ip = client_ip(request.headers(), context.remote_addr);
    request.extensions_mut().insert(context);
    next.run(request).await
}

fn parse_endpoint(value: &str) -> Option<SocketAddr> {
    value.parse().ok().or_else(|| {
        value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(value)
            .parse::<IpAddr>()
            .ok()
            .map(|ip| SocketAddr::new(ip, 0))
    })
}

fn truncate_header(headers: &mut HeaderMap, name: &'static str, values: &[String]) {
    headers.remove(name);
    for value in values.iter().take(values.len().saturating_sub(1)) {
        if let Ok(value) = HeaderValue::from_str(value) {
            headers.append(name, value);
        }
    }
}

fn comma_values(headers: &HeaderMap, name: &str) -> Vec<String> {
    let mut result = Vec::new();
    for value in headers.get_all(name) {
        let Ok(value) = value.to_str() else { continue };
        let mut start = 0;
        let mut quoted = false;
        for (index, character) in value
            .char_indices()
            .chain(std::iter::once((value.len(), ',')))
        {
            if character == '"' {
                quoted = !quoted;
            }
            if index == value.len() || (character == ',' && !quoted) {
                let part = value[start..index].trim_matches([' ', '\t']);
                let part = part
                    .strip_prefix('"')
                    .and_then(|part| part.strip_suffix('"'))
                    .unwrap_or(part);
                if !part.is_empty() {
                    result.push(part.to_owned());
                }
                start = index + 1;
            }
        }
    }
    result
}

fn client_ip(headers: &HeaderMap, remote: Option<SocketAddr>) -> Option<String> {
    for name in ["x-original-forwarded-for", "x-forwarded-for"] {
        let value = headers
            .get_all(name)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect::<Vec<_>>()
            .join(",");
        if let Some(first) = value
            .split(',')
            .find(|value| !value.is_empty())
            .map(str::trim)
            && !first.is_empty()
        {
            return Some(first.to_owned());
        }
    }
    remote.map(|remote| match remote.ip() {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map_or_else(|| ip.to_string(), |ip| ip.to_string()),
        ip => ip.to_string(),
    })
}
