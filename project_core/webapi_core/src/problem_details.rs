//! MVC 输入格式不受支持时使用 ProblemDetails，不混入业务错误协议。
use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};

use axum::{
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub(crate) fn unsupported_media_type(headers: &HeaderMap) -> Response {
    #[derive(Serialize)]
    struct Problem {
        r#type: &'static str,
        title: &'static str,
        status: u16,
        #[serde(rename = "traceId")]
        trace_id: String,
    }
    let mut body = Vec::new();
    if common::json_helper::write_controller_json(
        &mut body,
        &Problem {
            r#type: "https://tools.ietf.org/html/rfc9110#section-15.5.16",
            title: "Unsupported Media Type",
            status: 415,
            trace_id: trace_id(headers),
        },
    )
    .is_err()
    {
        return crate::error::internal_error_response("ProblemDetails 序列化失败");
    }
    let mut response = (
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        [(
            header::CONTENT_TYPE,
            "application/problem+json; charset=utf-8",
        )],
        body,
    )
        .into_response();
    response
        .extensions_mut()
        .insert(crate::response_format::OutputKind::Problem);
    response
}

fn trace_id(headers: &HeaderMap) -> String {
    // 源宿主将 StringValues 合并后交给 W3C propagator；空值不占分隔符。
    // .NET 10 默认不读取旧 Request-Id，也不把它作为非法 traceparent 的回退。
    let mut value = String::new();
    for header in headers.get_all("traceparent") {
        let Ok(part) = header.to_str() else {
            return w3c_id(None);
        };
        if !part.is_empty() {
            if !value.is_empty() {
                value.push(',');
            }
            value.push_str(part);
        }
    }
    let parent = parse_parent(&value);
    if parent.is_some() && value.len() > 55 {
        // Propagator 接受较新版本的扩展，Activity 仅将 55 字符父 ID 识别为 W3C。
        return hierarchical_id(&value);
    }
    w3c_id(parent)
}

fn w3c_id(parent: Option<(&str, u8)>) -> String {
    let (trace, flags) = parent
        .map(|(trace, flags)| (trace.to_owned(), flags))
        .unwrap_or_else(|| (uuid::Uuid::new_v4().simple().to_string(), 0));
    let span = uuid::Uuid::new_v4().simple().to_string();
    format!("00-{trace}-{}-{flags:02x}", &span[..16])
}

fn parse_parent(value: &str) -> Option<(&str, u8)> {
    let bytes = value.as_bytes();
    if bytes.len() < 55
        || (&bytes[..2] == b"00" && bytes.len() != 55)
        || (bytes.len() > 55 && bytes[55] != b'-')
        || bytes[2] != b'-'
        || bytes[35] != b'-'
        || bytes[52] != b'-'
        || !lower_hex(&bytes[..2])
        || &bytes[..2] == b"ff"
        || !lower_hex(&bytes[3..35])
        || bytes[3..35].iter().all(|byte| *byte == b'0')
        || !lower_hex(&bytes[36..52])
        || bytes[36..52].iter().all(|byte| *byte == b'0')
        || !lower_hex(&bytes[53..55])
    {
        return None;
    }
    // Activity 将最高位用于内部状态，其对外 ID 只保留低七位。
    let flags = u8::from_str_radix(&value[53..55], 16).ok()? & 0x7f;
    Some((&value[3..35], flags))
}

fn hierarchical_id(value: &str) -> String {
    const MAX_LENGTH: usize = 1024;
    let mut parent = format!("|{value}");
    if !matches!(parent.as_bytes().last(), Some(b'.' | b'_')) {
        parent.push('.');
    }
    let suffix = format!("{:x}", next_hierarchical_id());
    if parent.len() + suffix.len() < MAX_LENGTH {
        return format!("{parent}{suffix}_");
    }

    // 只在已有层级边界截断，给八位随机后缀和 # 留出空间。
    // HeaderValue::to_str 仅接受可见 ASCII 和制表符，字节位置等于源字符位置。
    let mut trim = parent.len().min(MAX_LENGTH - 9);
    while trim > 1 && !matches!(parent.as_bytes()[trim - 1], b'.' | b'_') {
        trim -= 1;
    }
    if trim > 1 {
        return format!(
            "{}{:08x}#",
            &parent[..trim],
            uuid::Uuid::new_v4().as_fields().0
        );
    }

    // 超长且没有可保留层级时生成新根；进程后缀保持稳定。
    static PROCESS_SUFFIX: OnceLock<u64> = OnceLock::new();
    let process_suffix = PROCESS_SUFFIX.get_or_init(|| uuid::Uuid::new_v4().as_u64_pair().0);
    format!("|{:x}-{process_suffix:x}.", next_hierarchical_id())
}

fn next_hierarchical_id() -> u64 {
    static COUNTER: OnceLock<AtomicU64> = OnceLock::new();
    COUNTER
        .get_or_init(|| AtomicU64::new(u64::from(uuid::Uuid::new_v4().as_fields().0)))
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1)
}

fn lower_hex(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}
