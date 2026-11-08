use crate::{DEFAULT_METHOD_LOCK_LEASE_SECONDS, error_summary};
use distributed_lock::{DistributedLock, LockGuard};
use std::{future::Future, time::Duration};
use tokio::time::{Instant, sleep};

/// 缓存回源和并发限制共用的租约生命周期。
/// 正常结束等待解锁，取消或 unwind 由句柄的 Drop 启动释放。
#[doc(hidden)]
pub async fn run_with_renewed_lock<F: Future>(
    lock: &dyn DistributedLock,
    guard: LockGuard,
    expiry: Duration,
    method: &str,
    operation: F,
) -> F::Output {
    let expiry = if expiry.is_zero() {
        Duration::from_secs(DEFAULT_METHOD_LOCK_LEASE_SECONDS)
    } else {
        expiry
    };
    let result = {
        tokio::pin!(operation);
        tokio::select! {
            result = &mut operation => result,
            () = renew_lease(lock, &guard, expiry, method) => operation.await,
        }
    };
    guard.release().await;
    result
}

/// 按租约三分之一（最多二十秒）续期，失败在安全期限内短间隔重试。
async fn renew_lease(
    lock: &dyn DistributedLock,
    guard: &LockGuard,
    expiry: Duration,
    method: &str,
) {
    let interval = (expiry / 3).min(Duration::from_secs(20));
    let retry = (interval / 4).min(Duration::from_secs(1));
    let deadline = expiry - interval;
    let mut last_success = Instant::now();
    let mut delay = interval;
    loop {
        sleep(delay).await;
        let renewal = lock.renew(guard, expiry).await;
        if matches!(renewal, Ok(true)) {
            last_success = Instant::now();
            delay = interval;
        } else if let Some(remaining) = deadline.checked_sub(last_success.elapsed())
            && !remaining.is_zero()
        {
            delay = retry.min(remaining);
        } else {
            tracing::error!(method, event = "method_lease_lost", error = ?renewal.err().map(|error| error_summary(&error)), "方法锁续期失败，业务继续执行");
            return;
        }
    }
}
