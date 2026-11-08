//! 对应控制器返回的字符串结果，与文件、健康端点的直接文本响应区分。
use axum::{
    body::{Body, to_bytes},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};

/// 参与共享文本格式化的字符串结果。
pub struct Text(pub String);

impl IntoResponse for Text {
    fn into_response(self) -> Response {
        let Some(object) = snapshot(&self.0) else {
            return crate::error::internal_error_response("字符串响应序列化失败");
        };
        with_snapshot(self.0.into_response(), object)
    }
}

/// 已序列化为文本的字符串结果，例如 UEditor 返回的 JSON 字符串。
/// 调用方必须保证正文为完整 UTF-8 文本，不用于文件、下载或任意二进制响应。
pub async fn from_response(response: Response) -> Response {
    let (parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, usize::MAX).await else {
        return crate::error::internal_error_response("字符串响应读取失败");
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return crate::error::internal_error_response("字符串响应编码无效");
    };
    let Some(object) = snapshot(text) else {
        return crate::error::internal_error_response("字符串响应序列化失败");
    };
    with_snapshot(Response::from_parts(parts, Body::from(bytes)), object)
}

fn snapshot(text: &str) -> Option<crate::object_response::ObjectResponse> {
    let mut json = Vec::new();
    common::json_helper::write_controller_json(&mut json, text).ok()?;
    Some(crate::object_response::ObjectResponse {
        json: json.into(),
        internal_json: crate::object_response::ObjectResponse::internal_json(text),
    })
}

fn with_snapshot(
    mut response: Response,
    object: crate::object_response::ObjectResponse,
) -> Response {
    response.extensions_mut().insert(object);
    response
        .extensions_mut()
        .insert(crate::response_format::OutputKind::String);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}
