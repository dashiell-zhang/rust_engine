use crate::{LockError, LockGuard};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// 分布式锁的统一操作接口
///
/// 支持三种操作：阻塞获取、非阻塞尝试获取、续期
/// 具体实现由 `InMemoryLock`（单进程）和 `RedisLock`（分布式）提供
///
/// # 信号量
///
/// `semaphore` 参数控制同一 key 可同时被持有的名额数：
/// - 传 `1` 为标准互斥锁，同一时刻只允许一个持有者
/// - 传 `N > 1` 为信号量语义，允许最多 N 个并发持有者
///
/// # 默认过期时长
///
/// `expiry` 传 `Duration::ZERO` 时，所有实现均默认使用 60 秒
#[async_trait]
pub trait DistributedLock: Send + Sync {
    /// 阻塞式获取锁
    ///
    /// 在 `expiry` 超时前以 100ms 为间隔持续重试
    /// 成功后返回 `LockGuard`，drop 时自动释放
    /// 超时后返回 `LockError::Timeout`
    ///
    /// - `key`：锁名称，同一 key 同时只允许 `semaphore` 个持有者
    /// - `expiry`：锁的失效时长，同时也是等待获取的最长时间；传 `Duration::ZERO` 时默认 60 秒
    /// - `semaphore`：可同时持有锁的名额数量；传 `0` 时返回 `LockError::InvalidArgument`
    async fn lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<LockGuard, LockError>;

    /// 非阻塞式尝试获取锁
    ///
    /// 立即尝试一次，成功返回 `Some(LockGuard)`，所有名额已被占用则返回 `None`
    /// 不会进行重试，也不会等待
    ///
    /// - `key`：锁名称
    /// - `expiry`：获取成功后锁的失效时长；传 `Duration::ZERO` 时默认 60 秒
    /// - `semaphore`：可同时持有锁的名额数量
    async fn try_lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<Option<LockGuard>, LockError>;

    /// 续期锁
    ///
    /// 将已持有的锁的失效时长延长至新的 `expiry`
    /// 返回 `true` 表示续期成功；`false` 表示锁已过期或句柄无效
    ///
    /// 内存锁通过 `LockGuard` 内部的续期回调实现
    /// Redis 锁通过 Lua 脚本校验 value 后执行 PEXPIRE 实现
    ///
    /// - `handle`：由 `lock` 或 `try_lock` 返回的句柄
    /// - `expiry`：新的失效时长；传 `Duration::ZERO` 时默认 60 秒
    async fn renew(&self, handle: &LockGuard, expiry: Duration) -> Result<bool, LockError>;
}

#[async_trait]
impl<T> DistributedLock for Arc<T>
where
    T: DistributedLock + ?Sized,
{
    /// 将阻塞式获取转发给 Arc 内部的锁实现
    async fn lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<LockGuard, LockError> {
        self.as_ref().lock(key, expiry, semaphore).await
    }

    /// 将非阻塞式获取转发给 Arc 内部的锁实现
    async fn try_lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<Option<LockGuard>, LockError> {
        self.as_ref().try_lock(key, expiry, semaphore).await
    }

    /// 将续期操作转发给 Arc 内部的锁实现
    async fn renew(&self, handle: &LockGuard, expiry: Duration) -> Result<bool, LockError> {
        self.as_ref().renew(handle, expiry).await
    }
}

#[async_trait]
impl<T> DistributedLock for &T
where
    T: DistributedLock + ?Sized,
{
    /// 将阻塞式获取转发给引用指向的锁实现
    async fn lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<LockGuard, LockError> {
        (**self).lock(key, expiry, semaphore).await
    }

    /// 将非阻塞式获取转发给引用指向的锁实现
    async fn try_lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<Option<LockGuard>, LockError> {
        (**self).try_lock(key, expiry, semaphore).await
    }

    /// 将续期操作转发给引用指向的锁实现
    async fn renew(&self, handle: &LockGuard, expiry: Duration) -> Result<bool, LockError> {
        (**self).renew(handle, expiry).await
    }
}
