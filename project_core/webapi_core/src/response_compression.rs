use std::{convert::Infallible, future::ready};

use axum::{
    body::Body,
    extract::Request,
    http::{Extensions, HeaderMap, HeaderValue, StatusCode, Version, header},
    middleware::Next,
    response::Response,
};
use tower::{ServiceExt, service_fn};
use tower_http::{CompressionLevel, compression::Compression};

const MIME_TYPES: &[&str] = &[
    "text/plain",
    "text/css",
    "application/javascript",
    "text/javascript",
    "text/html",
    "application/xml",
    "text/xml",
    "application/json",
    "text/json",
    "application/wasm",
];

/// 非 Development 宿主共用；协商规则与源宿主一致，正文仍交给流式编码器。
pub(crate) async fn response_compression(request: Request, next: Next) -> Response {
    let accept: Vec<_> = request
        .headers()
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .cloned()
        .collect();
    let mut response = next.run(request).await;
    if !accept.iter().any(|value| !value.is_empty()) || !compressible(response.headers()) {
        return response;
    }

    let headers = response.headers_mut();
    if !headers.get_all(header::VARY).iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("Accept-Encoding"))
        })
    }) {
        headers.append(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    }
    let Some(encoding) = select_encoding(&accept) else {
        return response;
    };

    // Tower 默认移除 Accept-Ranges，源宿主保留它；编码后原正文摘要不可沿用。
    let ranges: Vec<_> = headers
        .get_all(header::ACCEPT_RANGES)
        .iter()
        .cloned()
        .collect();
    headers.remove("content-md5");
    let mut response = Some(response);
    let service = service_fn(move |_: Request| {
        // oneshot 只调用一次服务，不重试，也不再次执行业务处理器。
        ready(Ok::<_, Infallible>(
            response.take().expect("single response service"),
        ))
    });
    let mut encoding_request = Request::new(Body::empty());
    encoding_request
        .headers_mut()
        .insert(header::ACCEPT_ENCODING, HeaderValue::from_static(encoding));
    let compressed = Compression::new(service)
        .compress_when(|_: StatusCode, _: Version, _: &HeaderMap, _: &Extensions| true)
        .quality(CompressionLevel::Precise(1))
        .oneshot(encoding_request)
        .await;
    let mut response = match compressed {
        Ok(response) => response.map(Body::new),
        Err(error) => match error {},
    };
    for value in ranges {
        response.headers_mut().append(header::ACCEPT_RANGES, value);
    }
    response
}

fn compressible(headers: &HeaderMap) -> bool {
    if headers.contains_key(header::CONTENT_ENCODING) || headers.contains_key(header::CONTENT_RANGE)
    {
        return false;
    }
    let Some(content_type) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let mime = content_type
        .split_once(';')
        .map_or(content_type, |(mime, _)| mime.trim());
    MIME_TYPES
        .iter()
        .any(|allowed| mime.eq_ignore_ascii_case(allowed))
}

fn select_encoding(values: &[HeaderValue]) -> Option<&'static str> {
    // 同质量优先 br、gzip、identity；每个格式只保留首个正质量候选。
    let mut qualities = [None; 3];
    'headers: for value in values {
        let Ok(value) = value.to_str() else { continue };
        for item in value.split(',') {
            // 源 TryParseList 会逐字符跳过非法内容，再尝试后续合法值。
            let Some((name, quality)) = item
                .char_indices()
                .find_map(|(index, _)| parse_item(&item[index..]))
            else {
                continue;
            };
            if quality == 0 {
                continue;
            }
            if name == "*" {
                qualities[0].get_or_insert(quality);
                qualities[1].get_or_insert(quality);
                break 'headers;
            }
            for (index, encoding) in ["br", "gzip", "identity"].iter().enumerate() {
                if name.eq_ignore_ascii_case(encoding) {
                    qualities[index].get_or_insert(quality);
                }
            }
        }
    }
    let mut selected = None;
    let mut best_quality = 0;
    for (index, quality) in qualities.into_iter().enumerate() {
        if let Some(quality) = quality.filter(|quality| *quality > best_quality) {
            best_quality = quality;
            selected = match index {
                0 => Some("br"),
                1 => Some("gzip"),
                _ => None,
            };
        }
    }
    selected
}

pub(crate) fn parse_item(item: &str) -> Option<(&str, u32)> {
    let item = item.trim_matches([' ', '\t']);
    let length = item
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(byte))
        .count();
    if length == 0 {
        return None;
    }
    let name = &item[..length];
    let rest = item[length..].trim_start_matches([' ', '\t']);
    if rest.is_empty() {
        return Some((name, 100_000_000));
    }
    let rest = rest.strip_prefix(';')?.trim_start_matches([' ', '\t']);
    let rest = rest
        .strip_prefix(['q', 'Q'])?
        .trim_start_matches([' ', '\t']);
    let quality = rest.strip_prefix('=')?.trim_start_matches([' ', '\t']);
    let (integer, fraction) = quality.split_once('.').unwrap_or((quality, ""));
    if !matches!(integer, "0" | "1")
        || fraction.len() > 8
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u32>().ok()? * 10_u32.pow(8 - fraction.len() as u32)
    };
    let quality = if integer == "1" {
        100_000_000 + fraction
    } else {
        fraction
    };
    (quality <= 100_000_000).then_some((name, quality))
}
