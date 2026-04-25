use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use distributed_cache::{CacheEntryOptions, DistributedCache};
use moka::future::Cache;
use serde::{Serialize, de::DeserializeOwned};

use crate::error::HybridCacheError;

/// L1+L2 混合缓存
///
/// L1 为进程内内存缓存（基于 moka），L2 为外部分布式缓存（实现 `DistributedCache` 的任意后端）
/// 读取顺序：L1 命中直接返回，L1 未命中查询 L2 并回填 L1，L2 未命中调用工厂方法后同时写入 L1 与 L2
pub struct HybridCache {
    /// L1 进程内内存缓存，以 JSON 字节形式存储序列化后的值
    l1: Cache<String, Vec<u8>>,

    /// L2 分布式缓存后端
    l2: Arc<dyn DistributedCache>,
}

impl HybridCache {
    /// 创建混合缓存实例
    ///
    /// - `l2`：分布式缓存后端
    /// - `l1_capacity`：L1 最大条目数
    /// - `l1_ttl_seconds`：L1 缓存的统一存活时长（秒）
    pub fn new(l2: Arc<dyn DistributedCache>, l1_capacity: u64, l1_ttl_seconds: u64) -> Self {
        let l1 = Cache::builder()
            .max_capacity(l1_capacity)
            .time_to_live(Duration::from_secs(l1_ttl_seconds))
            .build();

        Self { l1, l2 }
    }

    /// 获取缓存值，若不存在则通过工厂方法生成并同时写入 L1 与 L2
    ///
    /// - `key`：缓存键
    /// - `l2_ttl_seconds`：L2 缓存的存活时长（秒），须大于 0
    /// - `factory`：缓存未命中时的异步生产者，负责生成并返回原始值
    pub async fn get_or_create_json<T, F, Fut>(
        &self,
        key: &str,
        l2_ttl_seconds: u64,
        factory: F,
    ) -> Result<T, HybridCacheError>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, HybridCacheError>>,
    {
        // L1 命中：直接反序列化并返回
        if let Some(bytes) = self.l1.get(key).await {
            return Ok(serde_json::from_slice::<T>(&bytes)?);
        }

        // L2 命中：回填 L1 后返回
        if let Some(bytes) = self.l2.get_bytes(key).await? {
            let value: T = serde_json::from_slice(&bytes)?;
            self.l1.insert(key.to_string(), bytes).await;
            return Ok(value);
        }

        // L1 与 L2 均未命中：调用工厂方法，结果同时写入 L1 与 L2
        let value = factory().await?;
        let bytes = serde_json::to_vec(&value)?;

        let l2_options = CacheEntryOptions::relative(Duration::from_secs(l2_ttl_seconds));
        self.l2.set_bytes(key, bytes.clone(), l2_options).await?;
        self.l1.insert(key.to_string(), bytes).await;

        Ok(value)
    }

    /// 仅从 L1 中获取缓存值，不触发 L2 查询与工厂方法
    pub async fn get_l1_json<T>(&self, key: &str) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let bytes = self.l1.get(key).await?;
        serde_json::from_slice::<T>(&bytes).ok()
    }

    /// 使指定键的缓存在 L1 与 L2 中同时失效
    pub async fn remove(&self, key: &str) -> Result<(), HybridCacheError> {
        self.l1.invalidate(key).await;
        self.l2.remove(key).await?;

        Ok(())
    }

    /// 批量使指定键集合的缓存在 L1 与 L2 中同时失效
    pub async fn remove_many(&self, keys: &[&str]) -> Result<(), HybridCacheError> {
        for key in keys {
            self.remove(key).await?;
        }

        Ok(())
    }
}
