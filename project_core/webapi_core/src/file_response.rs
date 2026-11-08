//! 文件接口的物理下载响应，与公开静态目录的范围下载策略分开。
use std::{path::Path, time::SystemTime};

use application_model::file::models::FileLocalContentDto;
use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use tokio_util::io::ReaderStream;

use crate::error::AppError;

/// 获取源框架的文件 MIME 类型；普通下载对未知扩展名回退 ZIP 类型。
pub fn content_type(path: &str, require_known: bool) -> Result<&'static str, AppError> {
    let known = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(crate::static_content_types::content_type);
    match known {
        Some(value) => Ok(value),
        None if !require_known => {
            Ok(crate::static_content_types::content_type("zip").expect("ZIP MIME 映射必须存在"))
        }
        None => Err(common::app_error::AppError::internal("未找到文件扩展名的 MIME 类型").into()),
    }
}

/// 流式返回原文件，使用附件名和修改日期，不启用范围下载或 ETag。
pub async fn physical_file(
    file: FileLocalContentDto,
    headers: &HeaderMap,
    require_known: bool,
) -> Result<Response, AppError> {
    let content_type = content_type(&file.path, require_known)?;
    let stream = tokio::fs::File::open(&file.local_path)
        .await
        .map_err(|error| common::app_error::AppError::internal(format!("打开文件失败: {error}")))?;
    let metadata = stream.metadata().await.map_err(|error| {
        common::app_error::AppError::internal(format!("读取文件信息失败: {error}"))
    })?;
    if !metadata.is_file() {
        return Err(common::app_error::AppError::internal("目标不是文件").into());
    }
    let modified = metadata.modified().map_err(|error| {
        common::app_error::AppError::internal(format!("读取文件修改日期失败: {error}"))
    })?;
    let seconds = chrono::DateTime::<chrono::Utc>::from(modified).timestamp();
    let mut state = 0;
    for (name, unchanged, changed) in [
        (header::IF_MODIFIED_SINCE, 1, 2),
        (header::IF_UNMODIFIED_SINCE, 2, 3),
    ] {
        if let Some(date) = headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| httpdate::parse_http_date(v).ok())
            && date <= SystemTime::now()
        {
            state = state.max(
                if seconds <= chrono::DateTime::<chrono::Utc>::from(date).timestamp() {
                    unchanged
                } else {
                    changed
                },
            );
        }
    }
    let status = match state {
        1 => StatusCode::NOT_MODIFIED,
        3 => StatusCode::PRECONDITION_FAILED,
        _ => StatusCode::OK,
    };
    let body = if status == StatusCode::OK {
        Body::from_stream(ReaderStream::new(stream))
    } else {
        Body::empty()
    };
    let mut response = (status, body).into_response();
    let result_headers = response.headers_mut();
    let date = chrono::DateTime::<chrono::Utc>::from(modified)
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    result_headers.insert(header::LAST_MODIFIED, HeaderValue::from_str(&date).unwrap());
    if status == StatusCode::OK {
        result_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        result_headers.insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&metadata.len().to_string()).unwrap(),
        );
        if !file.file_name.is_empty() {
            result_headers.insert(
                header::CONTENT_DISPOSITION,
                attachment_name(&file.file_name),
            );
        }
    }
    Ok(response)
}

/// 缩放结果为无附件名的 PNG 内存响应。
pub fn png(bytes: Vec<u8>) -> Response {
    let length = bytes.len();
    let mut response = Body::from(bytes).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    response
}

fn attachment_name(name: &str) -> HeaderValue {
    // HTTP 文件名保留 ASCII 回退与 UTF-8 filename*；控制字符不会进入响应头。
    let sanitized: String = name
        .encode_utf16()
        .map(|unit| {
            if (0x20..0x7f).contains(&unit) {
                char::from(unit as u8)
            } else {
                '_'
            }
        })
        .collect();
    let token = |byte: u8| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte);
    let fallback = if sanitized.bytes().all(token) {
        sanitized
    } else {
        format!(
            "\"{}\"",
            sanitized.replace('\\', "\\\\").replace('"', "\\\"")
        )
    };
    let mut encoded = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(encoded, "%{byte:02X}").unwrap();
        }
    }
    HeaderValue::from_str(&format!(
        "attachment; filename={fallback}; filename*=UTF-8''{encoded}"
    ))
    .unwrap()
}
