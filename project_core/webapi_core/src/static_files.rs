//! 公开 Web 根目录；已匹配的业务端点不经过文件服务。
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub(crate) async fn serve(root: PathBuf, mut request: Request) -> Response {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return StatusCode::NOT_FOUND.into_response();
    }
    let decoded = crate::request_path::value(request.uri(), request.extensions());
    // 默认不提供点文件、点目录，也不接受父目录路径段。
    let relative = decoded.trim_start_matches('/');
    if relative.is_empty()
        || relative.ends_with('/')
        || relative.contains(['\\', '\0'])
        || relative.split('/').any(|part| part.starts_with('.'))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = root.join(relative);
    let Some(content_type) = path
        .extension()
        .and_then(|value| value.to_str())
        .and_then(crate::static_content_types::content_type)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let metadata = match tokio::fs::metadata(&path).await {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let Ok(modified) = metadata.modified() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let seconds = chrono::DateTime::<chrono::Utc>::from(modified).timestamp();
    let modified = if seconds >= 0 {
        UNIX_EPOCH + Duration::from_secs(seconds as u64)
    } else {
        UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
    };
    // Windows FILETIME 的 1601 年纪元与秒级最后修改时间。
    let file_time = ((seconds as i128 + 11_644_473_600) * 10_000_000) as u64;
    let etag = format!("\"{:x}\"", file_time ^ metadata.len());
    let last_modified = httpdate::fmt_http_date(modified);
    let precondition = precondition(request.headers(), &etag, modified);
    if precondition == 3 {
        return StatusCode::PRECONDITION_FAILED.into_response();
    }
    if precondition == 1 {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        file_headers(response.headers_mut(), content_type, &etag, &last_modified);
        return response;
    }

    let headers = request.headers_mut();
    // 条件判断统一在这里完成，文件服务仅负责流式读取与单范围传输。
    for name in [
        header::IF_MATCH,
        header::IF_NONE_MATCH,
        header::IF_MODIFIED_SINCE,
        header::IF_UNMODIFIED_SINCE,
    ] {
        headers.remove(name);
    }
    let if_range_matches = headers
        .get(header::IF_RANGE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|value| {
            let value = value.trim();
            if value.starts_with('"') || value.starts_with("W/") {
                value == etag
            } else {
                httpdate::parse_http_date(value).map_or(true, |date| modified <= date)
            }
        });
    headers.remove(header::IF_RANGE);
    let range = normalized_range(headers, metadata.len());
    headers.remove(header::RANGE);
    if request.method() == Method::GET && if_range_matches {
        match range {
            FileRange::Unsatisfiable => {
                let mut response = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
                response.headers_mut().insert(
                    header::CONTENT_RANGE,
                    HeaderValue::from_str(&format!("bytes */{}", metadata.len())).unwrap(),
                );
                return response;
            }
            FileRange::Bytes(start, end) => {
                request.headers_mut().insert(
                    header::RANGE,
                    HeaderValue::from_str(&format!("bytes={start}-{end}")).unwrap(),
                );
            }
            FileRange::Ignore => {}
        }
    }
    let mut response = match ServeFile::new(path).oneshot(request).await {
        Ok(response) => response.map(Body::new),
        Err(error) => match error {},
    };
    if response.status().is_success() {
        file_headers(response.headers_mut(), content_type, &etag, &last_modified);
    }
    response
}

fn file_headers(headers: &mut HeaderMap, content_type: &'static str, etag: &str, modified: &str) {
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::ETAG, HeaderValue::from_str(etag).unwrap());
    headers.insert(
        header::LAST_MODIFIED,
        HeaderValue::from_str(modified).unwrap(),
    );
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
}

// 与源静态文件管线相同的优先级：未指定 < 未修改 < 继续处理 < 前置条件失败。
fn precondition(headers: &HeaderMap, etag: &str, modified: SystemTime) -> u8 {
    let mut result = 0;
    for (name, matched, unmatched) in [(header::IF_MATCH, 2, 3), (header::IF_NONE_MATCH, 1, 2)] {
        let tags: Vec<_> = headers
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|value| value.split(','))
            .map(str::trim)
            .filter(|value| {
                *value == "*"
                    || value.starts_with('"') && value.ends_with('"')
                    || value.starts_with("W/\"") && value.ends_with('"')
            })
            .collect();
        if !tags.is_empty() {
            let matches = tags.iter().any(|tag| *tag == "*" || *tag == etag);
            result = result.max(if matches { matched } else { unmatched });
        }
    }
    for (name, unmodified, changed) in [
        (header::IF_MODIFIED_SINCE, 1, 2),
        (header::IF_UNMODIFIED_SINCE, 2, 3),
    ] {
        if let Some(date) = headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| httpdate::parse_http_date(v).ok())
            && date <= SystemTime::now()
        {
            result = result.max(if modified <= date {
                unmodified
            } else {
                changed
            });
        }
    }
    result
}

enum FileRange {
    Ignore,
    Unsatisfiable,
    Bytes(u64, u64),
}

fn normalized_range(headers: &HeaderMap, length: u64) -> FileRange {
    let values = headers.get_all(header::RANGE);
    if values.iter().count() != 1 {
        return FileRange::Ignore;
    }
    let Some(value) = values.iter().next().and_then(|v| v.to_str().ok()) else {
        return FileRange::Ignore;
    };
    let Some((unit, range)) = value.trim().split_once('=') else {
        return FileRange::Ignore;
    };
    if !unit.trim().eq_ignore_ascii_case("bytes") || range.contains(',') {
        return FileRange::Ignore;
    }
    let Some((start, end)) = range.trim().split_once('-') else {
        return FileRange::Ignore;
    };
    let number = |value: &str| -> Option<u64> {
        if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        value.parse::<i64>().ok().map(|value| value as u64)
    };
    let start = start.trim();
    let end = end.trim();
    if start.is_empty() {
        let Some(suffix) = number(end) else {
            return FileRange::Ignore;
        };
        if length == 0 || suffix == 0 {
            return FileRange::Unsatisfiable;
        }
        return FileRange::Bytes(length.saturating_sub(suffix), length - 1);
    }
    let Some(start) = number(start) else {
        return FileRange::Ignore;
    };
    let end = if end.is_empty() {
        None
    } else {
        let Some(end) = number(end) else {
            return FileRange::Ignore;
        };
        if end < start {
            return FileRange::Ignore;
        }
        Some(end)
    };
    if start >= length {
        return FileRange::Unsatisfiable;
    }
    FileRange::Bytes(start, end.unwrap_or(length - 1).min(length - 1))
}
