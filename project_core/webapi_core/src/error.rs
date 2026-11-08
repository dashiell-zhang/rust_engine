use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use common::app_error::{AppError as ApplicationError, AppErrorKind};
use futures_util::FutureExt;
use serde::Serialize;
use std::panic::AssertUnwindSafe;
use tracing::warn;

/// 标记未处理异常；这类响应不能继续带上此前的续签结果。
#[derive(Clone, Copy)]
pub(crate) struct UnhandledError;

/// 错误正文不携带 ObjectResponse 标记，不能进入对象缓存或生成对象 ETag。
pub(crate) fn json_error_response(status: StatusCode, err_msg: impl Into<String>) -> Response {
    let body = common::json_helper::object_to_json(&ErrBody {
        err_msg: err_msg.into(),
    })
    .unwrap_or_else(|_| "{\"errMsg\":\"系统全局内部异常\"}".to_string());
    (
        status,
        [(
            header::CONTENT_TYPE,
            crate::object_response::JSON_CONTENT_TYPE,
        )],
        body,
    )
        .into_response()
}

/// 与源宿主的生产全局异常响应保持一致，开发环境保留诊断信息。
pub(crate) fn internal_error_response(message: &str) -> Response {
    tracing::error!(target: "app.error", error = %message, "未处理的请求异常");
    let (status, err_msg) = if crate::environment::is_development() {
        (StatusCode::INTERNAL_SERVER_ERROR, message.to_string())
    } else {
        (StatusCode::BAD_REQUEST, "系统全局内部异常".to_string())
    };
    let mut response = json_error_response(status, err_msg);
    if !crate::environment::is_development() {
        use axum::http::{HeaderValue, header};
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache,no-store"),
        );
        response
            .headers_mut()
            .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
        response
            .headers_mut()
            .insert(header::EXPIRES, HeaderValue::from_static("-1"));
    }
    response.extensions_mut().insert(UnhandledError);
    response
}

/// 捕获请求 future 中尚未转换为应用异常的 panic。
pub async fn exception_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    match AssertUnwindSafe(next.run(request)).catch_unwind().await {
        Ok(response) => response,
        Err(_) => internal_error_response("请求处理发生未捕获异常"),
    }
}

/// 业务异常响应体
#[derive(Serialize)]
pub struct ErrBody {
    /// 错误描述
    #[serde(rename = "errMsg")]
    pub err_msg: String,
}

/// WebAPI 业务异常包装
pub struct AppError {
    /// 应用层业务异常
    inner: ApplicationError,
}

impl From<ApplicationError> for AppError {
    /// 从应用层业务异常创建宿主异常
    fn from(value: ApplicationError) -> Self {
        Self { inner: value }
    }
}

/// WebAPI 业务异常响应适配
impl IntoResponse for AppError {
    /// 输出统一的业务错误响应
    fn into_response(self) -> Response {
        let status_code = match self.inner.kind() {
            AppErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            AppErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
            AppErrorKind::Conflict => StatusCode::CONFLICT,
            AppErrorKind::Internal | AppErrorKind::Cancelled => {
                return internal_error_response(self.inner.message());
            }
        };
        let err_msg = self.inner.message().to_string();

        warn!(target: "app.error", status_code = %status_code, err_msg = %err_msg, "业务异常");

        json_error_response(status_code, err_msg)
    }
}
