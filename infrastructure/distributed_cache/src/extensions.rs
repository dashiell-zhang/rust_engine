use crate::{CacheEntryOptions, CacheError, DistributedCache};
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

/// 分布式缓存的易用扩展方法
#[async_trait]
pub trait DistributedCacheExt: DistributedCache {
    /// 设置字符串缓存
    async fn set_string(&self, key: &str, value: &str) -> Result<(), CacheError> {
        self.set_bytes(key, value.as_bytes().to_vec(), CacheEntryOptions::new())
            .await
    }

    /// 设置带过期配置的字符串缓存
    async fn set_string_with_options(
        &self,
        key: &str,
        value: &str,
        options: CacheEntryOptions,
    ) -> Result<(), CacheError> {
        self.set_bytes(key, value.as_bytes().to_vec(), options)
            .await
    }

    /// 设置 JSON 缓存
    async fn set_json<T>(&self, key: &str, value: &T) -> Result<(), CacheError>
    where
        T: Serialize + Send + Sync,
    {
        self.set_json_with_options(key, value, CacheEntryOptions::new())
            .await
    }

    /// 设置带过期配置的 JSON 缓存
    async fn set_json_with_options<T>(
        &self,
        key: &str,
        value: &T,
        options: CacheEntryOptions,
    ) -> Result<(), CacheError>
    where
        T: Serialize + Send + Sync,
    {
        self.set_bytes(key, serde_json::to_vec(value)?, options)
            .await
    }

    /// 获取字符串缓存
    async fn get_string(&self, key: &str) -> Result<Option<String>, CacheError> {
        match self.get_bytes(key).await? {
            Some(value) => Ok(Some(String::from_utf8(value)?)),
            None => Ok(None),
        }
    }

    /// 获取 JSON 缓存
    async fn get_json<T>(&self, key: &str) -> Result<Option<T>, CacheError>
    where
        T: DeserializeOwned + Send,
    {
        match self.get_bytes(key).await? {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    /// 忽略错误设置字符串缓存
    ///
    /// 适合对缓存失败不敏感的业务场景
    async fn try_set_string(&self, key: &str, value: &str, options: CacheEntryOptions) -> bool {
        self.set_string_with_options(key, value, options)
            .await
            .is_ok()
    }

    /// 忽略错误设置 JSON 缓存
    ///
    /// 适合对缓存失败不敏感的业务场景
    async fn try_set_json<T>(&self, key: &str, value: &T, options: CacheEntryOptions) -> bool
    where
        T: Serialize + Send + Sync,
    {
        self.set_json_with_options(key, value, options)
            .await
            .is_ok()
    }

    /// 忽略错误获取字符串缓存
    async fn try_get_string(&self, key: &str) -> Option<String> {
        self.get_string(key).await.ok().flatten()
    }

    /// 忽略错误获取 JSON 缓存
    async fn try_get_json<T>(&self, key: &str) -> Option<T>
    where
        T: DeserializeOwned + Send,
    {
        self.get_json(key).await.ok().flatten()
    }

    /// 忽略错误判断缓存是否存在
    async fn try_contains_key(&self, key: &str) -> bool {
        self.contains_key(key).await.unwrap_or(false)
    }
}

/// 为所有缓存实现自动补充易用扩展方法
impl<T> DistributedCacheExt for T where T: DistributedCache + Send + Sync + ?Sized {}
