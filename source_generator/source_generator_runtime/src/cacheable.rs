use std::future::Future;

use crate::*;

/// 执行属性宏的分布式缓存流程，锁覆盖二次读取、回源及写入。
#[doc(hidden)]
pub async fn invoke_cacheable<T, E>(
    cache: &dyn DistributedCache,
    lock: &dyn DistributedLock,
    method: &str,
    arguments: &str,
    ttl_seconds: u64,
    lock_expiry_seconds: u64,
    body: impl Future<Output = Result<T, E>>,
) -> Result<T, E>
where
    T: Serialize + DeserializeOwned,
{
    let key = cacheable_key(method, arguments);
    if let Some(value) = read(cache, &key, method).await {
        return Ok(value);
    }
    let expiry = Duration::from_secs(if lock_expiry_seconds == 0 {
        DEFAULT_CACHEABLE_LOCK_EXPIRY_SECONDS
    } else {
        lock_expiry_seconds
    });
    let guard = match lock
        .lock(&cacheable_lock_key(method, arguments), expiry, 1)
        .await
    {
        Ok(guard) => Some(guard),
        Err(error) => {
            log_cache_degraded(method, "stampede_lock", &error);
            None
        }
    };
    let has_lock = guard.is_some();
    let operation = async {
        if has_lock && let Some(value) = read(cache, &key, method).await {
            return Ok(value);
        }
        let value = body.await?;
        let write = async {
            let bytes = serde_json::to_vec(&value)?;
            cache
                .set_bytes(
                    &key,
                    bytes,
                    CacheEntryOptions::relative(Duration::from_secs(ttl_seconds)),
                )
                .await
        }
        .await;
        if let Err(error) = write {
            log_cache_degraded(method, "write", &error);
        }
        Ok(value)
    };
    if let Some(guard) = guard {
        run_with_renewed_lock(lock, guard, expiry, method, operation).await
    } else {
        operation.await
    }
}

async fn read<T: DeserializeOwned>(
    cache: &dyn DistributedCache,
    key: &str,
    method: &str,
) -> Option<T> {
    let result: Result<Option<T>, CacheError> = async {
        cache
            .get_bytes(key)
            .await?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(CacheError::from))
            .transpose()
    }
    .await;
    match result {
        Ok(value) => value,
        Err(error) => {
            log_cache_degraded(method, "read", &error);
            None
        }
    }
}
