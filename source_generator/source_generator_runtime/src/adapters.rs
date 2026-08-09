use crate::*;

/// 混合缓存引用适配接口
pub trait AsHybridCacheRef {
    /// 获取混合缓存引用
    fn as_hybrid_cache_ref(&self) -> &HybridCache;
}

impl AsHybridCacheRef for HybridCache {
    /// 返回混合缓存自身引用
    fn as_hybrid_cache_ref(&self) -> &HybridCache {
        self
    }
}

impl AsHybridCacheRef for Arc<HybridCache> {
    /// 返回 Arc 包装的混合缓存引用
    fn as_hybrid_cache_ref(&self) -> &HybridCache {
        self.as_ref()
    }
}

impl<T> AsHybridCacheRef for &T
where
    T: AsHybridCacheRef + ?Sized,
{
    /// 透传引用包装的混合缓存引用
    fn as_hybrid_cache_ref(&self) -> &HybridCache {
        (*self).as_hybrid_cache_ref()
    }
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

/// 获取混合缓存引用
pub fn hybrid_cache_ref<T>(cache: &T) -> &HybridCache
where
    T: AsHybridCacheRef + ?Sized,
{
    cache.as_hybrid_cache_ref()
}

/// 使用返回类型自身的相等语义校验缓存值往返结果并写入缓存
pub async fn set_cacheable_json<T>(
    cache: &HybridCache,

    key: &str,

    value: &T,

    ttl_seconds: u64,
) -> Result<(), HybridCacheError>
where
    T: Serialize + DeserializeOwned + PartialEq,
{
    cache
        .set_json_with_round_trip_validator(
            key,
            value,
            ttl_seconds,
            |original_value, round_trip_value| Ok(original_value == round_trip_value),
        )
        .await
}

/// 获取分布式锁引用
pub fn distributed_lock_ref<T>(lock: &T) -> &dyn DistributedLock
where
    T: AsDistributedLockRef + ?Sized,
{
    lock.as_distributed_lock_ref()
}
