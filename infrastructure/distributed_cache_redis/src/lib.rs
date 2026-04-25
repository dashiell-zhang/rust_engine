use async_trait::async_trait;
use distributed_cache::options::{CacheMetadata, duration_to_u64_ms, validate_key};
use distributed_cache::{CacheEntryOptions, CacheError, DistributedCache};
use redis::{AsyncCommands, aio::ConnectionManager};
use std::time::SystemTime;

/// 基于 Redis 的分布式缓存实现
///
/// 使用两个 Key 保存缓存内容与元数据
/// - `{prefix}:{key}:data` 原始字节值
/// - `{prefix}:{key}:meta` JSON 元数据，记录到期时间和滑动过期时长
///
/// 这样既能直接保存二进制值，也能在读取时支持滑动过期续期
pub struct RedisCache {
    /// Redis 连接管理器
    conn_mgr: ConnectionManager,

    /// Key 前缀
    prefix: String,
}

impl RedisCache {
    /// 创建 Redis 缓存实例
    pub async fn new(connection_str: &str, prefix: &str) -> Result<Self, CacheError> {
        let client = redis::Client::open(connection_str)
            .map_err(|error| CacheError::InternalError(error.to_string()))?;
        let conn_mgr = ConnectionManager::new(client)
            .await
            .map_err(|error| CacheError::InternalError(error.to_string()))?;

        Ok(Self {
            conn_mgr,
            prefix: prefix.to_string(),
        })
    }

    /// 构造缓存数据 Key
    fn data_key(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            format!("{key}:data")
        } else {
            format!("{}:{key}:data", self.prefix)
        }
    }

    /// 构造缓存元数据 Key
    fn meta_key(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            format!("{key}:meta")
        } else {
            format!("{}:{key}:meta", self.prefix)
        }
    }

    /// 删除数据和元数据 Key
    async fn delete_pair(&self, key: &str) -> Result<(), CacheError> {
        let mut conn = self.conn_mgr.clone();
        redis::pipe()
            .atomic()
            .cmd("DEL")
            .arg(self.data_key(key))
            .arg(self.meta_key(key))
            .query_async::<()>(&mut conn)
            .await
            .map_err(|error| CacheError::InternalError(error.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl DistributedCache for RedisCache {
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
            self.delete_pair(key).await?;
            return Ok(());
        }

        let data_key = self.data_key(key);
        let meta_key = self.meta_key(key);
        let meta_json = serde_json::to_vec(&metadata)?;
        let ttl_ms = metadata
            .remaining_ttl(now)
            .map(|ttl| duration_to_u64_ms(ttl, "缓存 TTL 超出支持范围"))
            .transpose()?;
        let mut conn = self.conn_mgr.clone();
        let mut pipe = redis::pipe();

        pipe.atomic()
            .cmd("SET")
            .arg(&data_key)
            .arg(value)
            .ignore()
            .cmd("SET")
            .arg(&meta_key)
            .arg(meta_json)
            .ignore();

        if let Some(ttl_ms) = ttl_ms {
            pipe.cmd("PEXPIRE").arg(&data_key).arg(ttl_ms).ignore();
            pipe.cmd("PEXPIRE").arg(&meta_key).arg(ttl_ms).ignore();
        }

        pipe.query_async::<()>(&mut conn)
            .await
            .map_err(|error| CacheError::InternalError(error.to_string()))?;

        Ok(())
    }

    /// 获取字节缓存
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, CacheError> {
        validate_key(key)?;

        let data_key = self.data_key(key);
        let meta_key = self.meta_key(key);
        let mut conn = self.conn_mgr.clone();

        let value: Option<Vec<u8>> = conn
            .get(&data_key)
            .await
            .map_err(|error| CacheError::InternalError(error.to_string()))?;

        let Some(value) = value else {
            let _: Result<i64, _> = conn.del(&meta_key).await;
            return Ok(None);
        };

        let metadata_raw: Option<Vec<u8>> = conn
            .get(&meta_key)
            .await
            .map_err(|error| CacheError::InternalError(error.to_string()))?;

        let mut metadata = match metadata_raw {
            Some(raw) => serde_json::from_slice::<CacheMetadata>(&raw)?,
            None => CacheMetadata::default(),
        };

        let now = SystemTime::now();

        if metadata.is_expired(now) {
            redis::pipe()
                .atomic()
                .cmd("DEL")
                .arg(&data_key)
                .arg(&meta_key)
                .query_async::<()>(&mut conn)
                .await
                .map_err(|error| CacheError::InternalError(error.to_string()))?;

            return Ok(None);
        }

        if let Some(ttl) = metadata.refresh_sliding(now)? {
            let ttl_ms = duration_to_u64_ms(ttl, "缓存 TTL 超出支持范围")?;
            let metadata_json = serde_json::to_vec(&metadata)?;

            redis::pipe()
                .atomic()
                .cmd("SET")
                .arg(&meta_key)
                .arg(metadata_json)
                .ignore()
                .cmd("PEXPIRE")
                .arg(&data_key)
                .arg(ttl_ms)
                .ignore()
                .cmd("PEXPIRE")
                .arg(&meta_key)
                .arg(ttl_ms)
                .ignore()
                .query_async::<()>(&mut conn)
                .await
                .map_err(|error| CacheError::InternalError(error.to_string()))?;
        }

        Ok(Some(value))
    }

    /// 删除缓存
    async fn remove(&self, key: &str) -> Result<(), CacheError> {
        validate_key(key)?;
        self.delete_pair(key).await
    }
}
