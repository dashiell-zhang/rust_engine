//! 表单文件识别由绑定、签名和参数快照共同使用。
use axum::http::{HeaderMap, header};
use common::http_client_helper::{ContentDisposition, parse_content_disposition};

pub(crate) fn read(headers: &HeaderMap) -> Result<Option<ContentDisposition>, String> {
    let value = headers
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| std::str::from_utf8(value.as_bytes()).ok())
        .unwrap_or_default();
    let disposition = parse_content_disposition(value)
        .ok_or_else(|| format!("Form section has invalid Content-Disposition value: {value}"))?;
    // 源 FormFeature 只读取精确的 form-data 类型，其他合法 disposition 被跳过。
    Ok((disposition.disposition_type == "form-data").then_some(disposition))
}

pub(crate) fn file_name(disposition: &ContentDisposition) -> Option<&str> {
    disposition
        .filename_star
        .as_deref()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            disposition
                .filename
                .as_deref()
                .filter(|name| !name.is_empty())
        })
}
