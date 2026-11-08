use crate::*;

/// 重试间隔等待
///
/// delay_ms 为 0 时直接返回，不产生任何调度开销
pub async fn retry_sleep(delay_ms: u64) {
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

/// 只有允许继续重试时才调用；次数耗尽应先返回业务错误。
pub fn check_retry_cancellation(token: &CancellationToken) -> Result<(), OperationCancelled> {
    cancellation::check(token)
}

/// 正数等待支持取消；零间隔保持原有的直接重试语义。
pub async fn retry_sleep_with_cancellation(
    delay_ms: u64,
    token: &CancellationToken,
) -> Result<(), OperationCancelled> {
    if delay_ms > 0 {
        token
            .run_until_cancelled(tokio::time::sleep(Duration::from_millis(delay_ms)))
            .await
            .ok_or(OperationCancelled)?;
    }
    Ok(())
}

/// 校验重试方法的声明参数类型实现 Copy
#[doc(hidden)]
pub fn assert_retry_parameter_type_copy<T>()
where
    T: Copy,
{
}

/// 校验重试方法的匿名 impl Trait 参数实现 Copy
#[doc(hidden)]
pub fn assert_retry_parameter_copy<T>(_: &T)
where
    T: Copy,
{
}

/// 将秒级重试等待时间转换为毫秒
pub fn retry_delay_ms_from_seconds(delay_seconds: u64) -> u64 {
    delay_seconds.saturating_mul(1000)
}
