use common::error_helper::CustomError;
use reqwest::{
    Client, Method, Request, Response, Url,
    cookie::{CookieStore, Jar},
    header::{self, HeaderValue},
};

/// 未注册名称的标准处理器：Cookie 与连接池同寿命，不自动解压。
pub(super) struct UnconfiguredTransport {
    cookies: Jar,
}

impl UnconfiguredTransport {
    pub(super) fn new() -> Self {
        Self {
            cookies: Jar::default(),
        }
    }

    pub(super) async fn send(
        &self,
        client: &Client,
        mut request: Request,
    ) -> Result<Response, CustomError> {
        for count in 0..=50 {
            let (mut next_request, replayable) = match request.try_clone() {
                Some(copy) => (copy, true),
                None => {
                    let body = request.body_mut().take();
                    let copy = request
                        .try_clone()
                        .ok_or_else(|| CustomError::new("无法复制重定向请求元数据"))?;
                    *request.body_mut() = body;
                    (copy, false)
                }
            };
            let current_url = request.url().clone();
            let current_method = request.method().clone();
            self.add_cookies(&mut request)?;
            let response = client
                .execute(request)
                .await
                .map_err(|error| CustomError::with_source("远程请求失败", error))?;
            self.cookies.set_cookies(
                &mut response.headers().get_all(header::SET_COOKIE).iter(),
                &current_url,
            );
            let Some(next_url) = redirect_url(&current_url, &response) else {
                return Ok(response);
            };
            if count == 50 {
                return Ok(response);
            }
            let force_get = match response.status().as_u16() {
                300..=302 => current_method == Method::POST,
                303 => current_method != Method::GET && current_method != Method::HEAD,
                _ => false,
            };
            if force_get {
                *next_request.method_mut() = Method::GET;
                *next_request.body_mut() = None;
                remove_content_headers(&mut next_request)?;
            } else if !replayable {
                return Err(CustomError::new("重定向要求重新发送正文，但请求流不可重放"));
            }
            // 即使重定向到同一主机，也清理显式认证头。
            next_request.headers_mut().remove(header::AUTHORIZATION);
            next_request
                .headers_mut()
                .remove(header::PROXY_AUTHORIZATION);
            *next_request.url_mut() = next_url;
            drop(response);
            request = next_request;
        }
        unreachable!("有限重定向循环在最后一次返回响应")
    }

    fn add_cookies(&self, request: &mut Request) -> Result<(), CustomError> {
        let Some(cookies) = self.cookies.cookies(request.url()) else {
            return Ok(());
        };
        let mut manual = request.headers().get_all(header::COOKIE).iter();
        let mut value = Vec::new();
        if let Some(first) = manual.next() {
            value.extend_from_slice(first.as_bytes());
            value.extend_from_slice(b"; ");
        }
        value.extend_from_slice(cookies.as_bytes());
        for cookie in manual {
            value.extend_from_slice(b"; ");
            value.extend_from_slice(cookie.as_bytes());
        }
        let value = HeaderValue::from_bytes(&value)
            .map_err(|error| CustomError::with_source("组合请求 Cookie 失败", error))?;
        request.headers_mut().insert(header::COOKIE, value);
        Ok(())
    }
}

fn redirect_url(current: &Url, response: &Response) -> Option<Url> {
    if !matches!(response.status().as_u16(), 300..=303 | 307 | 308) {
        return None;
    }
    let location = response.headers().get(header::LOCATION)?.to_str().ok()?;
    let mut next = current.join(location).ok()?;
    if current.scheme() == "https" && next.scheme() != "https" {
        return None;
    }
    if next.fragment().is_none_or(str::is_empty) {
        next.set_fragment(current.fragment());
    }
    Some(next)
}

fn remove_content_headers(request: &mut Request) -> Result<(), CustomError> {
    // reqwest 合并存储内容头与请求头，转换为无正文的 GET 时只移除内容部分。
    let content_headers = request
        .headers()
        .keys()
        .filter(|name| common::http_client_helper::is_content_header(name))
        .cloned()
        .collect::<Vec<_>>();
    for name in content_headers {
        request.headers_mut().remove(name);
    }
    let transfer_encodings = request
        .headers()
        .get_all(header::TRANSFER_ENCODING)
        .iter()
        .map(|value| {
            value
                .to_str()
                .map_err(|error| CustomError::with_source("Transfer-Encoding 请求头无效", error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let remaining = transfer_encodings
        .iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.eq_ignore_ascii_case("chunked"))
        .collect::<Vec<_>>()
        .join(", ");
    request.headers_mut().remove(header::TRANSFER_ENCODING);
    if !remaining.is_empty() {
        request.headers_mut().insert(
            header::TRANSFER_ENCODING,
            HeaderValue::from_str(&remaining)
                .map_err(|error| CustomError::with_source("Transfer-Encoding 请求头无效", error))?,
        );
    }
    Ok(())
}
