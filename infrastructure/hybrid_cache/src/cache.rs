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
        if let Some(value) = self.get_json(key).await? {
            return Ok(value);
        }

        let value = factory().await?;
        self.set_json(key, &value, l2_ttl_seconds).await?;

        Ok(value)
    }

    /// 尝试从 L1 或 L2 读取 JSON 缓存值
    ///
    /// L1 内容损坏时会清除本地副本并继续读取 L2
    /// L2 内容损坏时会尽力删除坏数据并返回反序列化错误
    pub async fn get_json<T>(&self, key: &str) -> Result<Option<T>, HybridCacheError>
    where
        T: DeserializeOwned,
    {
        if let Some(bytes) = self.l1.get(key).await {
            match serde_json::from_slice::<T>(&bytes) {
                Ok(value) => return Ok(Some(value)),
                Err(_) => self.l1.invalidate(key).await,
            }
        }

        let Some(bytes) = self.l2.get_bytes(key).await? else {
            return Ok(None);
        };

        match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => {
                self.l1.insert(key.to_string(), bytes).await;
                Ok(Some(value))
            }
            Err(error) => {
                self.l1.invalidate(key).await;
                let _ = self.l2.remove(key).await;
                Err(error.into())
            }
        }
    }

    /// 将 JSON 值写入 L2 并在成功后回填 L1
    ///
    /// 写入前会执行一次序列化与反序列化往返校验，避免缓存命中后返回语义不同的值
    pub async fn set_json<T>(
        &self,
        key: &str,
        value: &T,
        l2_ttl_seconds: u64,
    ) -> Result<(), HybridCacheError>
    where
        T: Serialize + DeserializeOwned,
    {
        const MAX_DEPTH: usize = 64;
        const MAX_ITEMS: usize = 10_000;
        const MAX_BYTES: usize = 1024 * 1024;

        if l2_ttl_seconds == 0 {
            return Err(HybridCacheError::L2("L2 缓存有效期必须大于 0".to_string()));
        }

        let bytes = serde_json::to_vec(value)?;

        if bytes.len() > MAX_BYTES {
            return Err(HybridCacheError::ValueLimit(format!(
                "JSON 超过 {MAX_BYTES} 字节限制"
            )));
        }

        let round_trip: T = serde_json::from_slice(&bytes)?;
        let mut original_items = 0usize;
        let original = canonicalize_json(
            serde_json::to_value(value)?,
            0,
            &mut original_items,
            MAX_DEPTH,
            MAX_ITEMS,
        )?;
        let mut restored_items = 0usize;
        let restored = canonicalize_json(
            serde_json::to_value(&round_trip)?,
            0,
            &mut restored_items,
            MAX_DEPTH,
            MAX_ITEMS,
        )?;

        if original != restored {
            return Err(HybridCacheError::RoundTrip);
        }

        let l2_options = CacheEntryOptions::relative(Duration::from_secs(l2_ttl_seconds));
        self.l2.set_bytes(key, bytes.clone(), l2_options).await?;
        self.l1.insert(key.to_string(), bytes).await;

        Ok(())
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

/// 递归规范化 JSON 对象字段顺序并限制结构复杂度
fn canonicalize_json(
    value: serde_json::Value,
    depth: usize,
    item_count: &mut usize,
    max_depth: usize,
    max_items: usize,
) -> Result<serde_json::Value, HybridCacheError> {
    if depth > max_depth {
        return Err(HybridCacheError::ValueLimit(format!(
            "JSON 超过 {max_depth} 层深度限制"
        )));
    }

    *item_count = item_count
        .checked_add(1)
        .ok_or_else(|| HybridCacheError::ValueLimit("JSON 项目计数溢出".to_string()))?;

    if *item_count > max_items {
        return Err(HybridCacheError::ValueLimit(format!(
            "JSON 超过 {max_items} 项限制"
        )));
    }

    match value {
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(|value| canonicalize_json(value, depth + 1, item_count, max_depth, max_items))
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        serde_json::Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));

            entries
                .into_iter()
                .map(|(key, value)| {
                    canonicalize_json(value, depth + 1, item_count, max_depth, max_items)
                        .map(|value| (key, value))
                })
                .collect::<Result<serde_json::Map<_, _>, _>>()
                .map(serde_json::Value::Object)
        }
        value => Ok(value),
    }
}
