use crate::body_limit::DEFAULT_BUFFERED_BODY_LIMIT_BYTES;
use axum::body::Bytes;
use common::string_helper::decode_bom_text as decode_request_text;
use flate2::read::MultiGzDecoder;
use std::io::Read;

/// 签名和请求参数快照共用的文本读取规则。
pub(crate) fn read_text(content_encoding: Option<&str>, body: &Bytes) -> Result<String, String> {
    if content_encoding.is_some_and(|value| value.eq_ignore_ascii_case("gzip")) {
        let mut decompressed = Vec::new();
        MultiGzDecoder::new(body.as_ref())
            .take(DEFAULT_BUFFERED_BODY_LIMIT_BYTES as u64 + 1)
            .read_to_end(&mut decompressed)
            .map_err(|_| "gzip 请求体读取失败".to_string())?;
        if decompressed.len() > DEFAULT_BUFFERED_BODY_LIMIT_BYTES {
            return Err("解压后的请求体过大".to_string());
        }
        return Ok(decode_request_text(&decompressed));
    }

    Ok(decode_request_text(body))
}

/// 签名文本遵循源 GetRequestBody，保留完整文本与 gzip/BOM 规则。
pub(crate) fn read_signature_text(
    content_encoding: Option<&str>,
    body: &Bytes,
) -> Result<String, String> {
    if content_encoding.is_some_and(|value| value.eq_ignore_ascii_case("gzip")) {
        let mut decompressed = Vec::new();
        MultiGzDecoder::new(body.as_ref())
            .read_to_end(&mut decompressed)
            .map_err(|_| "gzip 请求体读取失败".to_string())?;
        return Ok(decode_request_text(&decompressed));
    }
    Ok(decode_request_text(body))
}
