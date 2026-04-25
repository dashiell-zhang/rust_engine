use distributed_cache::{CacheEntryOptions, DistributedCache};
use distributed_lock::DistributedLock;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::settings::IdentifierGeneratorSettings;

/// 自定义纪元起点毫秒时间戳
const TW_EPOCH: u64 = 1_640_995_200_000;

/// 节点标识缓存键前缀
const CACHE_KEY_PREFIX: &str = "IdentifierGenerator-";

/// 节点标识注册锁名称
const REGISTER_LOCK_KEY: &str = "IdentifierGenerator";

/// 节点标识缓存续租时长
const SIGN_CACHE_EXPIRY: Duration = Duration::from_secs(60 * 60 * 24 * 2);

/// 节点标识刷新间隔
pub(crate) const SIGN_REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24);

/// 机器标识占用位数
const MACHINE_ID_BITS: u8 = 5;

/// 数据中心标识占用位数
const DATA_CENTER_ID_BITS: u8 = 5;

/// 同毫秒序列号占用位数
const SEQUENCE_BITS: u8 = 11;

/// 时间戳占用位数
const TIMESTAMP_BITS: u8 = 42;

/// 允许的最大机器标识
const MAX_MACHINE_ID: u64 = (1 << MACHINE_ID_BITS) - 1;

/// 允许的最大数据中心标识
const MAX_DATA_CENTER_ID: u64 = (1 << DATA_CENTER_ID_BITS) - 1;

/// 机器标识左移位数
const MACHINE_ID_SHIFT: u8 = SEQUENCE_BITS;

/// 数据中心标识左移位数
const DATA_CENTER_ID_SHIFT: u8 = SEQUENCE_BITS + MACHINE_ID_BITS;

/// 时间戳左移位数
const TIMESTAMP_LEFT_SHIFT: u8 = SEQUENCE_BITS + MACHINE_ID_BITS + DATA_CENTER_ID_BITS;

/// 序列号掩码
const SEQUENCE_MASK: u64 = (1 << SEQUENCE_BITS) - 1;

/// 标识生成器错误
#[derive(Debug)]
pub enum IdentifierGeneratorError {
    /// 机器标识无效
    InvalidMachineId,

    /// 数据中心标识无效
    InvalidDataCenterId,

    /// 系统时钟发生回拨
    ClockMovedBackward,

    /// 数据中心标识或机器标识异常
    InvalidSign,

    /// 节点标识注册失败
    RegisterSignFailed(String),
}

impl std::fmt::Display for IdentifierGeneratorError {
    /// 输出标识生成错误文本
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMachineId => write!(f, "Invalid machine ID"),
            Self::InvalidDataCenterId => write!(f, "Invalid data center ID"),
            Self::ClockMovedBackward => write!(f, "System clock moved backward"),
            Self::InvalidSign => write!(f, "Machine ID or data center ID is invalid"),
            Self::RegisterSignFailed(message) => write!(f, "Register sign failed: {message}"),
        }
    }
}

impl std::error::Error for IdentifierGeneratorError {}

/// 标识生成器内部状态
struct IdentifierGeneratorState {
    /// 机器标识
    machine_id: u64,

    /// 数据中心标识
    data_center_id: u64,

    /// 当前毫秒内序列号
    sequence: u64,

    /// 最近一次生成标识时的毫秒时间戳
    last_timestamp: i64,

    /// 当前节点占位缓存键
    registration_cache_key: Option<String>,
}

/// 雪花标识生成器
///
/// 基于 Snowflake 算法，生成 64 位有序唯一整型 ID
/// 组成结构：1 位符号位 + 42 位时间戳 + 5 位数据中心 ID + 5 位机器 ID + 11 位序列号
#[derive(Clone)]
pub struct IdentifierGenerator {
    /// 受互斥锁保护的生成器内部状态，保证并发生成时序列号与时间戳的一致性
    state: Arc<Mutex<IdentifierGeneratorState>>,
}

impl IdentifierGenerator {
    /// 创建雪花标识生成器
    pub fn new(machine_id: u64, data_center_id: u64) -> Result<Self, IdentifierGeneratorError> {
        if machine_id > MAX_MACHINE_ID {
            return Err(IdentifierGeneratorError::InvalidMachineId);
        }
        if data_center_id > MAX_DATA_CENTER_ID {
            return Err(IdentifierGeneratorError::InvalidDataCenterId);
        }

        Ok(Self {
            state: Arc::new(Mutex::new(IdentifierGeneratorState {
                machine_id,
                data_center_id,
                sequence: 0,
                last_timestamp: -1,
                registration_cache_key: None,
            })),
        })
    }

    /// 基于基础设施依赖创建标识生成器
    ///
    /// 当配置未显式提供数据中心标识或机器标识时
    /// 会像 NetEngine 一样通过分布式锁与分布式缓存自动分配一个可用组合
    pub async fn from_infrastructure(
        settings: &IdentifierGeneratorSettings,
        distributed_lock: Arc<dyn DistributedLock>,
        distributed_cache: Arc<dyn DistributedCache>,
    ) -> Result<Self, IdentifierGeneratorError> {
        if settings.has_fixed_sign() {
            return Self::new(
                settings
                    .machine_id
                    .ok_or(IdentifierGeneratorError::InvalidSign)?,
                settings
                    .data_center_id
                    .ok_or(IdentifierGeneratorError::InvalidSign)?,
            );
        }

        let _guard = distributed_lock
            .lock(REGISTER_LOCK_KEY, Duration::from_secs(60), 1)
            .await
            .map_err(|error| IdentifierGeneratorError::RegisterSignFailed(error.to_string()))?;

        let start = rand::random_range(0..1024u64);
        let mut allocated_sign = None;

        for offset in 0..1024u64 {
            let combined_number = (start + offset) % 1024;
            let data_center_id = (combined_number >> 5) & MAX_DATA_CENTER_ID;
            let machine_id = combined_number & MAX_MACHINE_ID;
            let cache_key = build_sign_cache_key(data_center_id, machine_id);

            let is_allocated = distributed_cache
                .contains_key(&cache_key)
                .await
                .map_err(|error| IdentifierGeneratorError::RegisterSignFailed(error.to_string()))?;

            if is_allocated {
                continue;
            }

            distributed_cache
                .set_bytes(
                    &cache_key,
                    Vec::new(),
                    CacheEntryOptions::relative(SIGN_CACHE_EXPIRY),
                )
                .await
                .map_err(|error| IdentifierGeneratorError::RegisterSignFailed(error.to_string()))?;

            allocated_sign = Some((machine_id, data_center_id, cache_key));
            break;
        }

        let Some((machine_id, data_center_id, cache_key)) = allocated_sign else {
            return Err(IdentifierGeneratorError::RegisterSignFailed(
                "available sign combinations exhausted".to_string(),
            ));
        };

        let generator = Self::new(machine_id, data_center_id)?;
        generator.set_registration_cache_key(Some(cache_key)).await;
        Ok(generator)
    }

    /// 更新机器标识与数据中心标识
    ///
    /// 用于在运行时动态调整节点身份，例如通过外部服务刷新机器编号以保证分布式唯一性
    pub async fn update_sign(
        &self,
        machine_id: u64,
        data_center_id: u64,
    ) -> Result<(), IdentifierGeneratorError> {
        if machine_id > MAX_MACHINE_ID {
            return Err(IdentifierGeneratorError::InvalidMachineId);
        }
        if data_center_id > MAX_DATA_CENTER_ID {
            return Err(IdentifierGeneratorError::InvalidDataCenterId);
        }

        let mut state = self.state.lock().await;
        state.machine_id = machine_id;
        state.data_center_id = data_center_id;

        Ok(())
    }

    /// 生成一个 Snowflake ID
    pub async fn get_id(&self) -> Result<i64, IdentifierGeneratorError> {
        let mut state = self.state.lock().await;
        let mut timestamp = current_timestamp_ms() as i64;

        if timestamp < state.last_timestamp {
            return Err(IdentifierGeneratorError::ClockMovedBackward);
        }

        if timestamp == state.last_timestamp {
            state.sequence = (state.sequence + 1) & SEQUENCE_MASK;
            if state.sequence == 0 {
                timestamp = wait_next_millis(state.last_timestamp).await as i64;
            }
        } else {
            state.sequence = 0;
        }

        state.last_timestamp = timestamp;

        let id = (((timestamp as u64) - TW_EPOCH) << TIMESTAMP_LEFT_SHIFT)
            | (state.data_center_id << DATA_CENTER_ID_SHIFT)
            | (state.machine_id << MACHINE_ID_SHIFT)
            | state.sequence;

        Ok(id as i64)
    }

    /// 通过 ID 还原其中的 UTC 时间
    pub fn get_time_by_id(id: i64) -> Result<SystemTime, &'static str> {
        let timestamp_part = ((id as u64) >> TIMESTAMP_LEFT_SHIFT) & ((1u64 << TIMESTAMP_BITS) - 1);
        let actual_timestamp = timestamp_part + TW_EPOCH;

        UNIX_EPOCH
            .checked_add(Duration::from_millis(actual_timestamp))
            .ok_or("Invalid timestamp derived from ID")
    }

    /// 获取指定时间开始的最小 ID（用于范围查询）
    pub fn get_min_id_by_time(time: SystemTime) -> Result<i64, &'static str> {
        let timestamp = time
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "Time cannot be before the UNIX epoch")?
            .as_millis() as u64;

        if timestamp < TW_EPOCH {
            return Err("Time cannot be before the custom TW_EPOCH");
        }

        let adjusted = timestamp - TW_EPOCH;
        if adjusted >= (1u64 << TIMESTAMP_BITS) {
            return Err("Timestamp exceeds the 42-bit capacity");
        }

        Ok((adjusted << TIMESTAMP_LEFT_SHIFT) as i64)
    }

    /// 获取当前是否需要后台刷新节点占位标记
    pub async fn requires_sign_refresh(&self) -> bool {
        self.state.lock().await.registration_cache_key.is_some()
    }

    /// 刷新当前节点占位标记
    pub async fn refresh_sign(
        &self,
        distributed_cache: &dyn DistributedCache,
    ) -> Result<(), IdentifierGeneratorError> {
        let cache_key = self.state.lock().await.registration_cache_key.clone();
        let Some(cache_key) = cache_key else {
            return Ok(());
        };

        distributed_cache
            .set_bytes(
                &cache_key,
                Vec::new(),
                CacheEntryOptions::relative(SIGN_CACHE_EXPIRY),
            )
            .await
            .map_err(|error| IdentifierGeneratorError::RegisterSignFailed(error.to_string()))
    }

    /// 获取当前节点占位缓存键
    pub async fn registration_cache_key(&self) -> Option<String> {
        self.state.lock().await.registration_cache_key.clone()
    }

    /// 设置当前节点占位缓存键
    pub async fn set_registration_cache_key(&self, cache_key: Option<String>) {
        self.state.lock().await.registration_cache_key = cache_key;
    }
}

/// 获取当前 Unix 毫秒时间戳
fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_millis(0))
        .as_millis() as u64
}

/// 等待进入下一毫秒
async fn wait_next_millis(last_timestamp: i64) -> u64 {
    let mut timestamp = current_timestamp_ms() as i64;
    while timestamp <= last_timestamp {
        tokio::time::sleep(Duration::from_millis(1)).await;
        timestamp = current_timestamp_ms() as i64;
    }
    timestamp as u64
}

/// 构建节点占位缓存键
fn build_sign_cache_key(data_center_id: u64, machine_id: u64) -> String {
    format!("{CACHE_KEY_PREFIX}{data_center_id}:{machine_id}")
}
