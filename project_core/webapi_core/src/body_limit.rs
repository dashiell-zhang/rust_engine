use axum::http::{HeaderMap, header::CONTENT_LENGTH};

/// 中间件允许完整缓冲的默认 body 上限。
///
/// 该上限约束参数快照、缓存、ETag、队列限制等完整缓冲操作；
/// 签名使用独立的内存/临时文件回放，文件上传采用流式读取。
pub const DEFAULT_BUFFERED_BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024;

/// 判断 Content-Length 是否明确超过缓冲上限。
pub fn content_length_exceeds(headers: &HeaderMap, limit: usize) -> bool {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|value| value > limit)
}

/// 判断请求或响应是否带有明确的 Content-Length。
pub fn has_content_length(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some()
}
