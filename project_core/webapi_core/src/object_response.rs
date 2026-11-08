use axum::{
    body::Bytes,
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub(crate) const JSON_CONTENT_TYPE: &str = "application/json; charset=utf-8";

tokio::task_local! {
    /// 仅缓存过滤器执行对象处理器时收集内部 JSON，不影响普通响应。
    pub(crate) static CACHE_SERIALIZATION: bool;
    /// ETag 与缓存分别声明需要内部快照，嵌套过滤器不能互相关闭。
    pub(crate) static ETAG_SERIALIZATION: bool;
}

/// 已序列化的对象结果；只保留值，不将响应头、文件或流当作可缓存对象。
#[derive(Clone)]
pub(crate) struct ObjectResponse {
    pub json: Bytes,
    pub internal_json: Option<Bytes>,
}

impl ObjectResponse {
    pub fn is_null(&self) -> bool {
        self.json.as_ref() == b"null"
    }

    pub(crate) fn internal_json<T: serde::Serialize + ?Sized>(value: &T) -> Option<Bytes> {
        if !CACHE_SERIALIZATION
            .try_with(|enabled| *enabled)
            .unwrap_or(false)
            && !ETAG_SERIALIZATION
                .try_with(|enabled| *enabled)
                .unwrap_or(false)
        {
            return None;
        }
        match common::json_helper::object_to_json(&value) {
            Ok(json) => Some(json.into()),
            Err(_) => {
                tracing::warn!(target: "webapi.object_response", "响应对象的内部序列化失败");
                None
            }
        }
    }
}

impl IntoResponse for ObjectResponse {
    fn into_response(self) -> Response {
        let mut response = if self.is_null() {
            // 空对象没有 JSON 正文或媒体类型，仍保留对象标记供缓存与 ETag 判断。
            StatusCode::NO_CONTENT.into_response()
        } else {
            (
                [(header::CONTENT_TYPE, JSON_CONTENT_TYPE)],
                self.json.clone(),
            )
                .into_response()
        };
        response
            .extensions_mut()
            .insert(crate::response_format::OutputKind::Json);
        response.extensions_mut().insert(self);
        response
    }
}

/// 在处理器完成状态码装配后应用空对象格式化，早于 CORS、压缩与请求日志。
pub(crate) async fn format_object_response(request: Request, next: Next) -> Response {
    let accept: Vec<_> = request
        .headers()
        .get_all(header::ACCEPT)
        .iter()
        .cloned()
        .collect();
    let accept_charset: Vec<_> = request
        .headers()
        .get_all(header::ACCEPT_CHARSET)
        .iter()
        .cloned()
        .collect();
    let mut response = next.run(request).await;
    if response
        .extensions()
        .get::<ObjectResponse>()
        .is_some_and(ObjectResponse::is_null)
    {
        // (StatusCode::OK, Json(None)) 会覆盖 IntoResponse 的初始状态；其他显式状态保留。
        if response.status() == StatusCode::OK {
            *response.status_mut() = StatusCode::NO_CONTENT;
        }
        if response.status() == StatusCode::NO_CONTENT {
            response.headers_mut().remove(header::CONTENT_LENGTH);
        }
    }
    let response = crate::response_format::apply(response, &accept);
    crate::response_charset::apply(response, &accept_charset)
}
