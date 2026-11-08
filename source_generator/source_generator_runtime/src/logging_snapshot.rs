use std::{any::Any, backtrace::Backtrace, fmt::Display, panic::AssertUnwindSafe};

use serde::Serialize;
use serde_json::{Value, json};

use crate::invocation_key::canonical_json::{CanonicalJsonValue, SerializationBudget};

/// 宏在调用点选择序列化能力；不要求不透明基础设施类型实现 Serialize。
#[doc(hidden)]
pub struct LogValue<'a, T: ?Sized>(pub &'a T);

#[doc(hidden)]
pub trait CaptureLogValue {
    fn capture_log_value(self) -> Value;
}

impl<T: ?Sized> CaptureLogValue for &LogValue<'_, T> {
    fn capture_log_value(self) -> Value {
        Value::String(std::any::type_name::<T>().into())
    }
}

impl<T: Display + ?Sized> CaptureLogValue for &&LogValue<'_, T> {
    fn capture_log_value(self) -> Value {
        display_snapshot(self.0)
    }
}

impl<T: Serialize + ?Sized> CaptureLogValue for &&&LogValue<'_, T> {
    fn capture_log_value(self) -> Value {
        serialized_snapshot(self.0)
            .unwrap_or_else(|| Value::String(std::any::type_name::<T>().into()))
    }
}

impl<T: Serialize + Display + ?Sized> CaptureLogValue for &&&&LogValue<'_, T> {
    fn capture_log_value(self) -> Value {
        serialized_snapshot(self.0).unwrap_or_else(|| display_snapshot(self.0))
    }
}

impl CaptureLogValue for &&&&&LogValue<'_, tokio_util::sync::CancellationToken> {
    fn capture_log_value(self) -> Value {
        Value::String("<cancellation-token>".into())
    }
}

fn serialized_snapshot<T: Serialize + ?Sized>(value: &T) -> Option<Value> {
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        let budget = SerializationBudget::for_snapshot();
        serde_json::to_value(CanonicalJsonValue::new(value, &budget, 0, 0))
    }))
    .ok()?
    .ok()
}

fn display_snapshot<T: Display + ?Sized>(value: &T) -> Value {
    Value::String(
        std::panic::catch_unwind(AssertUnwindSafe(|| value.to_string()))
            .unwrap_or_else(|_| "<serialization-failed>".into()),
    )
}

/// Result 错误保留具体类型、消息和直接原因；没有 Error 契约时保留 Debug 表示。
#[doc(hidden)]
pub struct LogError<'a, T: ?Sized>(pub &'a T);

#[doc(hidden)]
pub trait CaptureLogError {
    fn capture_log_error(self) -> Value;
}

impl<T: std::fmt::Debug + ?Sized> CaptureLogError for &LogError<'_, T> {
    fn capture_log_error(self) -> Value {
        exception_payload(
            std::any::type_name::<T>(),
            safe_text(|| format!("{:?}", self.0)),
            None,
        )
    }
}

impl<T: std::error::Error + ?Sized> CaptureLogError for &&LogError<'_, T> {
    fn capture_log_error(self) -> Value {
        let inner = std::panic::catch_unwind(AssertUnwindSafe(|| {
            self.0.source().map(|source| source.to_string())
        }))
        .ok()
        .flatten();
        exception_payload(
            std::any::type_name::<T>(),
            safe_text(|| self.0.to_string()),
            inner,
        )
    }
}

pub(crate) fn panic_payload(payload: &(dyn Any + Send)) -> Value {
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|message| (*message).into())
        })
        .unwrap_or_else(|| "<panic-payload>".into());
    exception_payload("panic", message, None)
}

pub(crate) fn exception_payload(source: &str, message: String, inner: Option<String>) -> Value {
    json!({
        "source": source,
        "message": message,
        // Rust Error 没有统一的原始抛出堆栈接口，此处明确捕获日志边界。
        "stackTrace": Backtrace::force_capture().to_string(),
        "innerSource": Value::Null,
        "innerMessage": inner,
        "innerStackTrace": Value::Null,
    })
}

fn safe_text(format: impl FnOnce() -> String) -> String {
    std::panic::catch_unwind(AssertUnwindSafe(format))
        .unwrap_or_else(|_| "<serialization-failed>".into())
}

/// 捕获异步调用的 unwind，调用方记录后必须恢复原始 panic。
#[doc(hidden)]
pub async fn catch_log_unwind<F: std::future::Future>(
    future: F,
) -> Result<F::Output, Box<dyn Any + Send>> {
    use futures_util::FutureExt;
    AssertUnwindSafe(future).catch_unwind().await
}
