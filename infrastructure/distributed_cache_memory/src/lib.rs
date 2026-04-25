use async_trait::async_trait;
use dashmap::DashMap;
use distributed_cache::options::{CacheMetadata, validate_key};
use distributed_cache::{CacheEntryOptions, CacheError, DistributedCache};
use std::sync::Arc;
use std::time::SystemTime;

/// 内存缓存记录
#[derive(Clone)]
struct CacheRecord {
    /// 缓存值
    value: Vec<u8>,

    /// 缓存元数据
    metadata: CacheMetadata,
}

/// 基于进程内内存的分布式缓存实现
///
/// 内部使用 `DashMap` 存储缓存记录，支持并发读写
/// 不支持跨进程共享，仅适用于单进程场景
pub struct InMemoryCache {
    /// 缓存记录表
    records: Arc<DashMap<String, CacheRecord>>,
}

impl InMemoryCache {
    /// 创建新的内存缓存实例
    pub fn new() -> Self {
        Self {
            records: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryCache {
    /// 创建默认内存缓存实例
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DistributedCache for InMemoryCache {
    /// 设置字节缓存
    async fn set_bytes(
        &self,
        key: &str,
        value: Vec<u8>,
        options: CacheEntryOptions,
    ) -> Result<(), CacheError> {
        validate_key(key)?;

        let now = SystemTime::now();
        let metadata = options.to_metadata(now)?;

        if metadata.is_expired(now) {
            self.records.remove(key);
            return Ok(());
        }

        self.records
            .insert(key.to_string(), CacheRecord { value, metadata });
        Ok(())
    }

    /// 获取字节缓存
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        validate_key(key)?;

        let now = SystemTime::now();
        let mut record = match self.records.get_mut(key) {
            Some(record) => record,
            None => return Ok(None),
        };

        if record.metadata.is_expired(now) {
            drop(record);
            self.records.remove(key);
            return Ok(None);
        }

        record.metadata.refresh_sliding(now)?;
        Ok(Some(record.value.clone()))
    }

    /// 删除缓存
    async fn remove(&self, key: &str) -> Result<(), CacheError> {
        validate_key(key)?;
        self.records.remove(key);
        Ok(())
    }
}
