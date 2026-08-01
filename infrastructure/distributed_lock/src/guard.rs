use std::time::Duration;

/// 分布式锁句柄
///
/// 通过 `DistributedLock::lock` 或 `DistributedLock::try_lock` 获取
/// 持有此结构体期间锁保持有效，离开作用域时自动触发释放逻辑
///
/// # 释放行为
///
/// `Drop` 实现会调用构造时注入的 `release` 回调，具体行为由各实现决定：
/// - `InMemoryLock`：同步归还信号量名额
/// - `RedisLock`：fire-and-forget 方式校验持有者令牌后删除锁
pub struct LockGuard {
    /// 锁的 Key
    ///
    /// Redis 实现中为含前缀、摘要和槽位序号的完整 key，格式为 `{prefix}{md5(key)} {slot}`
    /// 内存实现中为调用方传入的原始 key
    pub lock_key: String,

    /// 锁的标识值
    ///
    /// Redis 实现中为每次成功获取锁时生成的唯一令牌，供续期与释放 Lua 脚本校验所有权
    /// 内存锁不依赖此字段，值为 `"in-memory"`
    pub lock_value: String,

    /// 释放锁的回调函数，fire-and-forget 风格
    ///
    /// 在 `Drop::drop` 中调用，由各实现在构造时注入具体的释放逻辑
    release: Box<dyn Fn() + Send + Sync>,

    /// 内存锁专用的续期回调
    ///
    /// 内存锁通过此回调取消旧的过期任务并启动新的过期任务完成续期
    /// Redis 锁不使用此字段（续期由 `RedisLock::renew` 通过连接执行），值为 `None`
    pub local_renew: Option<Box<dyn Fn(Duration) -> bool + Send + Sync>>,
}

impl LockGuard {
    /// 构造锁句柄
    ///
    /// 由各锁实现在获取成功后调用，外部不直接使用此方法
    pub fn new(
        lock_key: String,
        lock_value: String,
        release: Box<dyn Fn() + Send + Sync>,
        local_renew: Option<Box<dyn Fn(Duration) -> bool + Send + Sync>>,
    ) -> Self {
        Self {
            lock_key,
            lock_value,
            release,
            local_renew,
        }
    }

    /// 获取锁的 Key
    pub fn lock_key(&self) -> &str {
        &self.lock_key
    }
}

impl Drop for LockGuard {
    /// drop 时自动调用释放回调，归还锁资源
    fn drop(&mut self) {
        (self.release)();
    }
}
