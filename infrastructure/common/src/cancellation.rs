//! 调用方控制的协作取消；取消会释放正在等待的 future 或流。
use std::future::Future;

pub use ::cancellation::{CancellationError, CancellationToken, OperationCancelled, check};
use futures_util::{StreamExt, stream::BoxStream};

use crate::app_error::AppError;

pub async fn run<T>(
    token: &CancellationToken,
    operation: impl Future<Output = Result<T, AppError>>,
) -> Result<T, AppError> {
    token
        .run_until_cancelled(operation)
        .await
        .ok_or_else(AppError::cancelled)?
}

/// 取消作为独立错误返回，不能把中断的流当作正常耗尽。
pub fn stream<'a, T: Send + 'a>(
    stream: BoxStream<'a, Result<T, AppError>>,
    token: CancellationToken,
) -> BoxStream<'a, Result<T, AppError>> {
    Box::pin(futures_util::stream::try_unfold(
        (stream, token),
        |(mut stream, token)| async move {
            match token.run_until_cancelled(stream.next()).await {
                Some(Some(item)) => Ok(Some((item?, (stream, token)))),
                Some(None) => Ok(None),
                None => Err(AppError::cancelled()),
            }
        },
    ))
}
