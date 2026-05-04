use std::{future::Future, time::Duration};

use tokio::time::sleep;

/// 重试延迟策略
pub enum RetryStrategy {
    /// 固定间隔，每次重试等待相同时长
    Fixed,

    /// 指数退避，每次重试的等待时长按倍数递增，上限为 max_delay
    Exponential {
        /// 退避倍数，推荐值 2.0
        multiplier: f64,
    },
}

/// 重试配置
pub struct RetryConfig {
    /// 最大尝试次数（含首次）
    pub max_attempts: u32,

    /// 首次重试前的等待时长
    pub initial_delay: Duration,

    /// 延迟上限，仅对指数退避有效
    pub max_delay: Duration,

    /// 延迟策略
    pub strategy: RetryStrategy,
}

impl RetryConfig {
    /// 创建固定间隔重试配置
    pub fn fixed(max_attempts: u32, delay: Duration) -> Self {
        Self {
            max_attempts,
            initial_delay: delay,
            max_delay: delay,
            strategy: RetryStrategy::Fixed,
        }
    }

    /// 创建指数退避重试配置
    pub fn exponential(max_attempts: u32, initial_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_attempts,
            initial_delay,
            max_delay,
            strategy: RetryStrategy::Exponential { multiplier: 2.0 },
        }
    }

    /// 计算第 n 次重试（从 0 起）的等待时长
    fn delay_for(&self, attempt: u32) -> Duration {
        match &self.strategy {
            RetryStrategy::Fixed => self.initial_delay,
            RetryStrategy::Exponential { multiplier } => {
                if !multiplier.is_finite() || *multiplier < 1.0 {
                    return self.initial_delay.min(self.max_delay);
                }

                let factor = multiplier.powi(attempt as i32);
                let millis = (self.initial_delay.as_millis() as f64 * factor) as u128;
                Duration::from_millis(millis.min(self.max_delay.as_millis()) as u64)
            }
        }
    }
}

/// 按配置重试异步操作，任意错误均触发重试
///
/// 达到最大尝试次数后仍失败时，返回最后一次的错误
///
/// # 示例
/// ```
/// use std::time::Duration;
/// use common::retry_helper::{RetryConfig, retry};
///
/// async fn send() -> Result<(), String> {
///     retry(&RetryConfig::fixed(3, Duration::from_millis(200)), || async {
///         // 执行可能失败的操作
///         Ok::<_, String>(())
///     }).await
/// }
/// ```
pub async fn retry<T, E, F, Fut>(config: &RetryConfig, f: F) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    retry_if(config, f, |_| true).await
}

/// 按配置自动重试异步操作，仅当 should_retry 返回 true 时才触发重试
///
/// 业务性错误（如参数校验失败）可通过 should_retry 排除，避免无意义重试
pub async fn retry_if<T, E, F, Fut, P>(config: &RetryConfig, f: F, should_retry: P) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    P: Fn(&E) -> bool,
{
    let max_attempts = config.max_attempts.max(1);
    let mut last_error: Option<E> = None;

    for attempt in 0..max_attempts {
        match f().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                let is_last = attempt + 1 >= max_attempts;
                let retryable = should_retry(&error);

                if is_last || !retryable {
                    return Err(error);
                }

                last_error = Some(error);
                let delay = config.delay_for(attempt);
                sleep(delay).await;
            }
        }
    }

    // 执行到这里说明 max_attempts == 0，理论上不会发生（上面已做 max(1) 保护）
    Err(last_error.expect("max_attempts 至少为 1"))
}
