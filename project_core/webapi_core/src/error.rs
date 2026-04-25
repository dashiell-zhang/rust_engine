use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use common::app_error::{AppError as ApplicationError, AppErrorKind};
use serde::Serialize;
use tracing::warn;

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
            AppErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let err_msg = self.inner.message().to_string();

        warn!(target: "app.error", status_code = %status_code, err_msg = %err_msg, "业务异常");

        (status_code, Json(ErrBody { err_msg })).into_response()
    }
}
