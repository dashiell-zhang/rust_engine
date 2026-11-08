use crate::*;

/// 方法行为共用的默认租约秒数。
pub const DEFAULT_METHOD_LOCK_LEASE_SECONDS: u64 = 60;

/// 缓存防击穿锁的默认有效期秒数
pub const DEFAULT_CACHEABLE_LOCK_EXPIRY_SECONDS: u64 = DEFAULT_METHOD_LOCK_LEASE_SECONDS;

/// 并发限制默认不使用参数生成锁键
pub const DEFAULT_CONCURRENCY_LIMIT_USE_PARAMETER: bool = false;

/// 并发限制默认等待获取锁
pub const DEFAULT_CONCURRENCY_LIMIT_IS_BLOCK: bool = false;

/// 并发限制默认锁过期秒数
pub const DEFAULT_CONCURRENCY_LIMIT_EXPIRY_SECONDS: u64 = 0;

/// 并发限制默认信号量数量
pub const DEFAULT_CONCURRENCY_LIMIT_SEMAPHORE: u32 = 1;

/// 重试行为选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryOptions {
    /// 最大重试次数
    pub max_retries: u32,

    /// 每次重试前的等待秒数
    pub delay_seconds: u64,
}

impl Default for RetryOptions {
    /// 返回与 NetEngine 对齐的默认重试配置
    fn default() -> Self {
        Self {
            max_retries: 3,
            delay_seconds: 0,
        }
    }
}

/// 缓存行为选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheableOptions {
    /// 缓存有效期秒数
    pub ttl_seconds: u64,
}

impl Default for CacheableOptions {
    /// 返回与 NetEngine 对齐的默认缓存配置
    fn default() -> Self {
        Self { ttl_seconds: 60 }
    }
}

/// 并发限制行为选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConcurrencyLimitOptions {
    /// 是否将参数加入锁键
    pub is_use_parameter: bool,

    /// 获取不到锁时是否直接返回错误
    pub is_block: bool,

    /// 锁过期秒数
    pub expiry_seconds: u64,

    /// 允许的最大并发数
    pub semaphore: u32,
}

impl Default for ConcurrencyLimitOptions {
    /// 返回与 NetEngine 对齐的默认并发限制配置
    fn default() -> Self {
        Self {
            is_use_parameter: DEFAULT_CONCURRENCY_LIMIT_USE_PARAMETER,
            is_block: DEFAULT_CONCURRENCY_LIMIT_IS_BLOCK,
            expiry_seconds: DEFAULT_CONCURRENCY_LIMIT_EXPIRY_SECONDS,
            semaphore: DEFAULT_CONCURRENCY_LIMIT_SEMAPHORE,
        }
    }
}
