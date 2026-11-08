use axum::{
    body::{Body, Bytes},
    http::header,
    response::{IntoResponse, Response},
};
use futures_util::{Stream, StreamExt, stream};
use serde::Serialize;

/// 按消费进度序列化 JSON 数组，不预先收集异步序列。
///
/// 流失败时终止响应体，不补写结束括号或将失败伪装成正常结束。
/// 响应不携带对象结果标记，因此不会进入对象响应缓存或 ETag。
pub struct JsonStream<S>(pub S);

impl<S, T, E> IntoResponse for JsonStream<S>
where
    S: Stream<Item = Result<T, E>> + Send + 'static,
    T: Serialize + Send + 'static,
    E: Into<axum::BoxError> + 'static,
{
    fn into_response(self) -> Response {
        let opening = stream::once(async { Ok::<_, axum::BoxError>(Bytes::from_static(b"[")) });
        let items = stream::try_unfold(
            (Some(Box::pin(self.0)), true),
            |(source, first)| async move {
                let Some(mut source) = source else {
                    return Ok::<_, axum::BoxError>(None);
                };
                match source.next().await {
                    Some(item) => {
                        let value = item.map_err(Into::into)?;
                        let mut bytes = Vec::new();
                        if !first {
                            bytes.push(b',');
                        }
                        common::json_helper::write_controller_json_array_item(&mut bytes, &value)?;
                        Ok(Some((Bytes::from(bytes), (Some(source), false))))
                    }
                    None => Ok(Some((Bytes::from_static(b"]"), (None, false)))),
                }
            },
        );
        let mut response = (
            [(
                header::CONTENT_TYPE,
                crate::object_response::JSON_CONTENT_TYPE,
            )],
            Body::from_stream(opening.chain(items)),
        )
            .into_response();
        response
            .extensions_mut()
            .insert(crate::response_format::OutputKind::Json);
        response
    }
}
