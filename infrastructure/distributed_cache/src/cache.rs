use crate::options::CacheMetadata;
use crate::{CacheEntryOptions, CacheError};
use async_trait::async_trait;
use std::sync::Arc;

/// 分布式缓存读取结果
#[derive(Debug, Clone)]
pub struct DistributedCacheEntry {
    /// 缓存值
    pub value: Vec<u8>,

    /// 缓存元数据
    pub metadata: CacheMetadata,
}

/// 分布式缓存的统一操作接口
#[async_trait]
pub trait DistributedCache: Send + Sync {
    /// 设置字节缓存
    ///
    /// `options` 用于控制相对过期 绝对过期 或滑动过期
    async fn set_bytes(
        &self,
        key: &str,
        value: Vec<u8>,
        options: CacheEntryOptions,
    ) -> Result<(), CacheError>;

    /// 获取包含元数据的字节缓存
    ///
    /// 若缓存不存在或已过期则返回 `None`
    /// 滑动过期的缓存会在读取成功后自动续期
    async fn get_entry(&self, key: &str) -> Result<Option<DistributedCacheEntry>, CacheError>;

    /// 获取字节缓存
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        Ok(self.get_entry(key).await?.map(|entry| entry.value))
    }

    /// 删除缓存
    async fn remove(&self, key: &str) -> Result<(), CacheError>;

    /// 判断缓存是否存在
    ///
    /// 默认通过 `get_bytes` 实现
    /// 因此滑动过期缓存会在命中时自动续期
    async fn contains_key(&self, key: &str) -> Result<bool, CacheError> {
        Ok(self.get_bytes(key).await?.is_some())
    }
}

#[async_trait]
impl<T> DistributedCache for Arc<T>
where
    T: DistributedCache + ?Sized,
{
    /// 转发设置字节缓存调用
    async fn set_bytes(
        &self,
        key: &str,
        value: Vec<u8>,
        options: CacheEntryOptions,
    ) -> Result<(), CacheError> {
        self.as_ref().set_bytes(key, value, options).await
    }

    /// 转发包含元数据的字节缓存读取调用
    async fn get_entry(&self, key: &str) -> Result<Option<DistributedCacheEntry>, CacheError> {
        self.as_ref().get_entry(key).await
    }

    /// 转发删除缓存调用
    async fn remove(&self, key: &str) -> Result<(), CacheError> {
        self.as_ref().remove(key).await
    }

    /// 转发判断缓存是否存在调用
    async fn contains_key(&self, key: &str) -> Result<bool, CacheError> {
        self.as_ref().contains_key(key).await
    }
}

#[async_trait]
impl<T> DistributedCache for &T
where
    T: DistributedCache + ?Sized,
{
    /// 转发设置字节缓存调用
    async fn set_bytes(
        &self,
        key: &str,
        value: Vec<u8>,
        options: CacheEntryOptions,
    ) -> Result<(), CacheError> {
        (*self).set_bytes(key, value, options).await
    }

    /// 转发包含元数据的字节缓存读取调用
    async fn get_entry(&self, key: &str) -> Result<Option<DistributedCacheEntry>, CacheError> {
        (*self).get_entry(key).await
    }

    /// 转发删除缓存调用
    async fn remove(&self, key: &str) -> Result<(), CacheError> {
        (*self).remove(key).await
    }

    /// 转发判断缓存是否存在调用
    async fn contains_key(&self, key: &str) -> Result<bool, CacheError> {
        (*self).contains_key(key).await
    }
}
