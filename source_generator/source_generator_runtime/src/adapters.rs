use crate::*;

/// 分布式缓存引用适配接口。
pub trait AsDistributedCacheRef {
    fn as_distributed_cache_ref(&self) -> &dyn DistributedCache;
}

impl<T: DistributedCache> AsDistributedCacheRef for T {
    fn as_distributed_cache_ref(&self) -> &dyn DistributedCache {
        self
    }
}

impl AsDistributedCacheRef for dyn DistributedCache {
    fn as_distributed_cache_ref(&self) -> &dyn DistributedCache {
        self
    }
}

/// 获取具体实现、Arc 或借用包装中的缓存引用。
pub fn distributed_cache_ref<T: AsDistributedCacheRef + ?Sized>(
    cache: &T,
) -> &dyn DistributedCache {
    cache.as_distributed_cache_ref()
}

/// 分布式锁引用适配接口
pub trait AsDistributedLockRef {
    /// 获取分布式锁引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock;
}

impl<T> AsDistributedLockRef for T
where
    T: DistributedLock,
{
    /// 返回具体分布式锁或透明包装的 trait object 引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock {
        self
    }
}

impl AsDistributedLockRef for dyn DistributedLock {
    /// 返回动态分布式锁自身引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock {
        self
    }
}

/// 获取分布式锁引用
pub fn distributed_lock_ref<T>(lock: &T) -> &dyn DistributedLock
where
    T: AsDistributedLockRef + ?Sized,
{
    lock.as_distributed_lock_ref()
}
