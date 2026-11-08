use crate::routes::MethodRouter;
use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::Response,
};
use base64::{Engine as _, engine::general_purpose};
use sha2::{Digest, Sha256};

/// 为单个路由挂载 ETag 中间件
pub fn etag_method_router<TState>(route: MethodRouter<TState>) -> MethodRouter<TState>
where
    TState: Clone + Send + Sync + 'static,
{
    route.map_action(|route| route.route_layer(axum::middleware::from_fn(etag_middleware)))
}

/// ETag 仅处理 GET 的非空对象结果，不读取文件或流式响应。
pub async fn etag_middleware(request: Request, next: Next) -> Response {
    if request.method() != axum::http::Method::GET {
        return next.run(request).await;
    }
    let incoming: Vec<_> = request
        .headers()
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .cloned()
        .collect();
    let mut response = crate::object_response::ETAG_SERIALIZATION
        .scope(true, next.run(request))
        .await;
    if response.status() != StatusCode::OK {
        return response;
    }
    let Some(object) = response
        .extensions()
        .get::<crate::object_response::ObjectResponse>()
    else {
        return response;
    };
    if object.is_null() {
        return response;
    }
    let Some(json) = &object.internal_json else {
        return crate::error::internal_error_response("ETag 对象序列化失败");
    };
    let digest = Sha256::digest(json);
    let value = format!("\"{}\"", general_purpose::STANDARD.encode(digest));
    let Ok(etag) = HeaderValue::from_str(&value) else {
        return response;
    };
    if incoming.len() == 1 && incoming[0] == etag {
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        *response.body_mut() = Body::empty();
        response.headers_mut().remove(header::CONTENT_TYPE);
        response.headers_mut().remove(header::CONTENT_LENGTH);
        response
            .extensions_mut()
            .remove::<crate::object_response::ObjectResponse>();
    } else {
        response.headers_mut().insert(header::ETAG, etag);
    }
    response
}
