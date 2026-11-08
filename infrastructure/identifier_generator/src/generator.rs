use crate::{layout, lease::RedisNodeLease, settings::IdentifierGeneratorSettings};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

const MAX_NODE_PART: u64 = 31;
const SEQUENCE_MASK: u64 = (1 << layout::SEQUENCE_BITS) - 1;

#[derive(Debug)]
pub enum IdentifierGeneratorError {
    InvalidMachineId,
    InvalidDataCenterId,
    InvalidSettings(String),
    ClockMovedBackward,
    TimestampOutOfRange,
    NodeLeaseInvalid,
    ShutDown,
    RegisterSignFailed(String),
}

impl std::fmt::Display for IdentifierGeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMachineId => write!(f, "机器编号必须介于 0 和 31 之间"),
            Self::InvalidDataCenterId => write!(f, "数据中心编号必须介于 0 和 31 之间"),
            Self::InvalidSettings(message) => write!(f, "ID 配置无效: {message}"),
            Self::ClockMovedBackward => write!(f, "系统时钟回拨，拒绝生成 ID"),
            Self::TimestampOutOfRange => write!(f, "系统时间超出雪花 ID 范围"),
            Self::NodeLeaseInvalid => write!(f, "节点租约尚未确认或已经失效，禁止生成 ID"),
            Self::ShutDown => write!(f, "ID 生成器已停止"),
            Self::RegisterSignFailed(message) => write!(f, "节点分配失败: {message}"),
        }
    }
}
impl std::error::Error for IdentifierGeneratorError {}

struct GeneratorState {
    sequence: u64,
    last_timestamp: i64,
}

struct GeneratorInner {
    machine_id: u64,
    data_center_id: u64,
    state: Mutex<GeneratorState>,
    lease: Option<RedisNodeLease>,
    stopped: AtomicBool,
}

/// 一个宿主复用同一个生成器；所有克隆共享序列、节点身份和租约生命周期。
#[derive(Clone)]
pub struct IdentifierGenerator {
    inner: Arc<GeneratorInner>,
}

impl IdentifierGenerator {
    /// 固定编号模式，由部署方确保所有实例的编号唯一。
    pub fn new(machine_id: u64, data_center_id: u64) -> Result<Self, IdentifierGeneratorError> {
        Self::create(machine_id, data_center_id, None)
    }

    fn create(
        machine_id: u64,
        data_center_id: u64,
        lease: Option<RedisNodeLease>,
    ) -> Result<Self, IdentifierGeneratorError> {
        if machine_id > MAX_NODE_PART {
            return Err(IdentifierGeneratorError::InvalidMachineId);
        }
        if data_center_id > MAX_NODE_PART {
            return Err(IdentifierGeneratorError::InvalidDataCenterId);
        }
        Ok(Self {
            inner: Arc::new(GeneratorInner {
                machine_id,
                data_center_id,
                lease,
                stopped: AtomicBool::new(false),
                state: Mutex::new(GeneratorState {
                    sequence: 0,
                    last_timestamp: -1,
                }),
            }),
        })
    }

    /// 自动节点使用共享 Redis，明确独立于宿主的业务缓存和锁前缀。
    pub async fn from_settings(
        settings: &IdentifierGeneratorSettings,
        fallback_redis_connection: &str,
    ) -> Result<Self, IdentifierGeneratorError> {
        settings.validate()?;
        if let (Some(machine), Some(data_center)) = (settings.machine_id, settings.data_center_id) {
            return Self::new(machine, data_center);
        }
        let connection = if settings.redis_connection.trim().is_empty() {
            fallback_redis_connection
        } else {
            &settings.redis_connection
        };
        if connection.trim().is_empty() {
            return Err(IdentifierGeneratorError::InvalidSettings(
                "自动节点需要共享 Redis 连接".into(),
            ));
        }
        let (node, lease) = RedisNodeLease::acquire(settings, connection).await?;
        Self::create(u64::from(node & 31), u64::from(node >> 5), Some(lease))
    }

    fn ensure_valid(&self) -> Result<(), IdentifierGeneratorError> {
        if self.inner.stopped.load(Ordering::Acquire) {
            return Err(IdentifierGeneratorError::ShutDown);
        }
        if let Some(lease) = &self.inner.lease {
            lease.ensure_valid()?;
        }
        Ok(())
    }

    /// 生成前后检查租约；等待下一个毫秒期间也不能越过租约有效期继续发号。
    pub async fn get_id(&self) -> Result<i64, IdentifierGeneratorError> {
        let mut state = self.inner.state.lock().await;
        self.ensure_valid()?;
        let mut timestamp = current_timestamp_ms()?;
        if timestamp < state.last_timestamp {
            return Err(IdentifierGeneratorError::ClockMovedBackward);
        }
        let sequence = if timestamp == state.last_timestamp {
            (state.sequence + 1) & SEQUENCE_MASK
        } else {
            0
        };
        if timestamp == state.last_timestamp && sequence == 0 {
            loop {
                tokio::time::sleep(Duration::from_millis(1)).await;
                self.ensure_valid()?;
                timestamp = current_timestamp_ms()?;
                if timestamp < state.last_timestamp {
                    return Err(IdentifierGeneratorError::ClockMovedBackward);
                }
                if timestamp > state.last_timestamp {
                    break;
                }
            }
        }
        let timestamp_part = layout::min_id_at_unix_millis(timestamp)
            .map_err(|_| IdentifierGeneratorError::TimestampOutOfRange)?;
        self.ensure_valid()?;
        // 仅在所有检查和异步等待成功后提交状态，取消等待不能把序列绕回已使用的值。
        state.last_timestamp = timestamp;
        state.sequence = sequence;
        Ok(timestamp_part
            | ((self.inner.data_center_id << (layout::SEQUENCE_BITS + layout::MACHINE_ID_BITS))
                as i64)
            | ((self.inner.machine_id << layout::SEQUENCE_BITS) as i64)
            | sequence as i64)
    }

    /// 显式停止所有克隆发号并终止续期；退出不释放 Redis 节点占用。
    pub async fn shutdown(&self) {
        // 与正在提交 ID 的操作串行，确保方法返回后不再发出 ID。
        self.inner.stopped.store(true, Ordering::Release);
        let _state = self.inner.state.lock().await;
        if let Some(lease) = &self.inner.lease {
            lease.shutdown().await;
        }
    }

    /// 通过 ID 还原其中的 UTC 时间
    pub fn get_time_by_id(id: i64) -> Result<SystemTime, &'static str> {
        let millis = layout::unix_millis_from_id(id);
        if millis >= 0 {
            UNIX_EPOCH.checked_add(Duration::from_millis(millis as u64))
        } else {
            UNIX_EPOCH.checked_sub(Duration::from_millis(millis.unsigned_abs()))
        }
        .ok_or("Invalid timestamp derived from ID")
    }

    /// 获取指定时间开始的最小 ID（允许纪元之前的分区边界）。
    pub fn get_min_id_by_time(time: SystemTime) -> Result<i64, &'static str> {
        let millis = match time.duration_since(UNIX_EPOCH) {
            Ok(value) => i64::try_from(value.as_millis()).map_err(|_| "Timestamp overflow")?,
            Err(error) => {
                // Unix 毫秒采用向下取整，保持负时间与 DateTimeOffset 一致。
                let nanos = error.duration().as_nanos();
                -i64::try_from(nanos.div_ceil(1_000_000)).map_err(|_| "Timestamp overflow")?
            }
        };
        layout::min_id_at_unix_millis(millis)
    }
}

fn current_timestamp_ms() -> Result<i64, IdentifierGeneratorError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|time| i64::try_from(time.as_millis()).ok())
        .ok_or(IdentifierGeneratorError::TimestampOutOfRange)
}
