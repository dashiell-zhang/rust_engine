use crate::CacheError;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 缓存过期配置
#[derive(Debug, Clone, Default)]
pub struct CacheEntryOptions {
    /// 过期策略
    pub expiry: Option<CacheExpiry>,
}

impl CacheEntryOptions {
    /// 创建无过期时间的缓存配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建相对过期时间配置
    pub fn relative(expiry: Duration) -> Self {
        Self {
            expiry: Some(CacheExpiry::Relative(expiry)),
        }
    }

    /// 创建滑动过期时间配置
    pub fn sliding(expiry: Duration) -> Self {
        Self {
            expiry: Some(CacheExpiry::Sliding(expiry)),
        }
    }

    /// 创建绝对过期时间配置
    pub fn absolute(expiry: SystemTime) -> Self {
        Self {
            expiry: Some(CacheExpiry::Absolute(expiry)),
        }
    }

    /// 生成内部元数据
    pub fn to_metadata(&self, now: SystemTime) -> Result<CacheMetadata, CacheError> {
        match &self.expiry {
            None => Ok(CacheMetadata::default()),
            Some(CacheExpiry::Relative(expiry)) => {
                ensure_duration_positive(*expiry, "relative expiration 必须大于 0")?;
                Ok(CacheMetadata::new(
                    Some(system_time_to_unix_ms(now + *expiry)?),
                    None,
                ))
            }
            Some(CacheExpiry::Sliding(expiry)) => {
                ensure_duration_positive(*expiry, "sliding expiration 必须大于 0")?;
                let expiry_ms = duration_to_u64_ms(*expiry, "sliding expiration 超出支持范围")?;
                Ok(CacheMetadata::new(
                    Some(system_time_to_unix_ms(now + *expiry)?),
                    Some(expiry_ms),
                ))
            }
            Some(CacheExpiry::Absolute(expiry)) => Ok(CacheMetadata::new(
                Some(system_time_to_unix_ms(*expiry)?),
                None,
            )),
        }
    }
}

/// 缓存过期策略
#[derive(Debug, Clone)]
pub enum CacheExpiry {
    /// 相对过期
    Relative(Duration),

    /// 滑动过期
    Sliding(Duration),

    /// 绝对过期
    Absolute(SystemTime),
}

/// 缓存元数据
///
/// 供 `distributed_cache_memory` 和 `distributed_cache_redis` 等提供者 crate 使用
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// 当前到期时间的 Unix 毫秒时间戳
    pub expires_at_unix_ms: Option<u64>,

    /// 滑动过期时长
    pub sliding_expiration_ms: Option<u64>,
}

impl CacheMetadata {
    /// 创建缓存元数据
    pub fn new(expires_at_unix_ms: Option<u64>, sliding_expiration_ms: Option<u64>) -> Self {
        Self {
            expires_at_unix_ms,
            sliding_expiration_ms,
        }
    }

    /// 判断当前是否已过期
    pub fn is_expired(&self, now: SystemTime) -> bool {
        self.expires_at_unix_ms.is_some_and(|expiry| {
            system_time_to_unix_ms(now).map_or(true, |current| current >= expiry)
        })
    }

    /// 获取剩余 TTL
    pub fn remaining_ttl(&self, now: SystemTime) -> Option<Duration> {
        let expiry = self.expires_at_unix_ms?;
        let current = system_time_to_unix_ms(now).ok()?;

        if current >= expiry {
            Some(Duration::ZERO)
        } else {
            Some(Duration::from_millis(expiry - current))
        }
    }

    /// 在滑动过期场景下刷新过期时间
    pub fn refresh_sliding(&mut self, now: SystemTime) -> Result<Option<Duration>, CacheError> {
        let expiry_ms = match self.sliding_expiration_ms {
            Some(value) => value,
            None => return Ok(None),
        };

        let new_expiry = now + Duration::from_millis(expiry_ms);
        self.expires_at_unix_ms = Some(system_time_to_unix_ms(new_expiry)?);
        Ok(Some(Duration::from_millis(expiry_ms)))
    }
}

/// 校验 key
pub fn validate_key(key: &str) -> Result<(), CacheError> {
    if key.is_empty() {
        return Err(CacheError::InvalidArgument {
            message: "key 不能为空".into(),
        });
    }

    Ok(())
}

/// 将 SystemTime 转为 Unix 毫秒
pub fn system_time_to_unix_ms(time: SystemTime) -> Result<u64, CacheError> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CacheError::InvalidArgument {
            message: "时间不能早于 Unix Epoch".into(),
        })?;
    duration_to_u64_ms(duration, "时间超出支持范围")
}

/// 将毫秒时长转换为 u64
pub fn duration_to_u64_ms(duration: Duration, message: &str) -> Result<u64, CacheError> {
    u64::try_from(duration.as_millis()).map_err(|_| CacheError::InvalidArgument {
        message: message.into(),
    })
}

/// 校验时长必须大于 0
pub fn ensure_duration_positive(duration: Duration, message: &str) -> Result<(), CacheError> {
    if duration.is_zero() {
        return Err(CacheError::InvalidArgument {
            message: message.into(),
        });
    }

    Ok(())
}
