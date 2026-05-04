use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::Response,
    routing::MethodRouter,
};
use base64::{Engine as _, engine::general_purpose};
use sha2::{Digest, Sha256};

use crate::body_limit::{
    DEFAULT_BUFFERED_BODY_LIMIT_BYTES, content_length_exceeds, has_content_length,
};

/// 为单个路由挂载 ETag 中间件
pub fn etag_method_router<TState>(route: MethodRouter<TState>) -> MethodRouter<TState>
where
    TState: Clone + Send + Sync + 'static,
{
    route.layer(axum::middleware::from_fn(etag_middleware))
}

/// ETag 中间件
pub async fn etag_middleware(request: Request, next: Next) -> Response {
    if request.method() != axum::http::Method::GET {
        return next.run(request).await;
    }

    let if_none_match = request.headers().get(header::IF_NONE_MATCH).cloned();
    let response = next.run(request).await;

    if response.status() != StatusCode::OK {
        return response;
    }

    if !has_content_length(response.headers())
        || content_length_exceeds(response.headers(), DEFAULT_BUFFERED_BODY_LIMIT_BYTES)
    {
        return response;
    }

    let (parts, body) = response.into_parts();

    let bytes = match axum::body::to_bytes(body, DEFAULT_BUFFERED_BODY_LIMIT_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };

    if bytes.is_empty() {
        return Response::from_parts(parts, Body::empty());
    }

    let digest = Sha256::digest(&bytes);
    let etag_value = format!("\"{}\"", general_purpose::STANDARD.encode(digest));

    let etag_header = match HeaderValue::from_str(&etag_value) {
        Ok(value) => value,
        Err(_) => return Response::from_parts(parts, Body::from(bytes)),
    };

    if let Some(if_none_match) = if_none_match
        && if_none_match == etag_header
    {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .body(Body::empty())
            .unwrap();
    }

    let mut response = Response::from_parts(parts, Body::from(bytes));
    response.headers_mut().insert(header::ETAG, etag_header);
    response
}
