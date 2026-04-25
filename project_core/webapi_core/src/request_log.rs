use std::time::Instant;

use axum::{extract::Request, middleware::Next, response::Response};

/// 请求日志中间件
pub async fn request_log_middleware(request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let uri = request.uri().to_string();
    let start_time = Instant::now();

    let response = next.run(request).await;

    let status = response.status().as_u16();
    let elapsed_ms = start_time.elapsed().as_millis() as u64;

    if status >= 500 {
        tracing::error!(
            target: "http.request",
            method = %method,
            uri = %uri,
            status,
            elapsed_ms,
            "HTTP 请求失败"
        );
    } else {
        tracing::info!(
            target: "http.request",
            method = %method,
            uri = %uri,
            status,
            elapsed_ms,
            "HTTP 请求完成"
        );
    }

    response
}
