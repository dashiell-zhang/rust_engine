use crate::{IdentifierGeneratorError as Error, IdentifierGeneratorSettings};
use redis::aio::ConnectionManager;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const CHECK_SCRIPT: &str = "if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('PTTL', KEYS[1]) else return -1 end";
const RENEW_SCRIPT: &str = "if redis.call('GET', KEYS[1]) == ARGV[1] and redis.call('PTTL', KEYS[1]) > 0 then redis.call('PEXPIRE', KEYS[1], ARGV[2]); return ARGV[2] else return -1 end";

struct LeaseState {
    valid_until: Mutex<Instant>,
    lost_owner: AtomicBool,
    stopped: AtomicBool,
}

impl LeaseState {
    fn ensure_valid(&self) -> Result<(), Error> {
        if self.stopped.load(Ordering::Acquire)
            || self.lost_owner.load(Ordering::Acquire)
            || Instant::now()
                >= *self
                    .valid_until
                    .lock()
                    .map_err(|_| Error::NodeLeaseInvalid)?
        {
            return Err(Error::NodeLeaseInvalid);
        }
        Ok(())
    }

    fn confirm(
        &self,
        requested_at: Instant,
        remaining: i64,
        safety_margin: Duration,
    ) -> Result<(), Error> {
        let remaining = u64::try_from(remaining).map_err(|_| Error::NodeLeaseInvalid)?;
        let conservative = Duration::from_millis(remaining).saturating_sub(safety_margin);
        let deadline = requested_at
            .checked_add(conservative)
            .ok_or(Error::NodeLeaseInvalid)?;
        *self
            .valid_until
            .lock()
            .map_err(|_| Error::NodeLeaseInvalid)? = deadline;
        Ok(())
    }
}

/// 续期任务仅持有状态，不持有租约自身，避免 Arc 循环让退出的节点一直占用。
pub(crate) struct RedisNodeLease {
    state: Arc<LeaseState>,
    renewal: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl RedisNodeLease {
    pub(crate) async fn acquire(
        settings: &IdentifierGeneratorSettings,
        connection: &str,
    ) -> Result<(u16, Self), Error> {
        let client = redis::Client::open(connection).map_err(|error| {
            Error::RegisterSignFailed(format!("Redis 地址无效: {:?}", error.kind()))
        })?;
        let mut manager = tokio::time::timeout(REQUEST_TIMEOUT, ConnectionManager::new(client))
            .await
            .map_err(|_| Error::RegisterSignFailed("连接共享 ID Redis 超时".into()))?
            .map_err(redis_error)?;
        let owner = uuid::Uuid::new_v4().simple().to_string();
        let start = rand::random_range(0..1024u16);
        let duration = settings.lease_duration_milliseconds;
        for offset in 0..1024u16 {
            let node = (start + offset) % 1024;
            let key = format!("{}:Node:{node}", settings.instance_name);
            let acquired: Option<String> = tokio::time::timeout(
                REQUEST_TIMEOUT,
                redis::cmd("SET")
                    .arg(&key)
                    .arg(&owner)
                    .arg("NX")
                    .arg("PX")
                    .arg(duration)
                    .query_async(&mut manager),
            )
            .await
            .map_err(|_| Error::RegisterSignFailed("节点占用结果未确认：Redis 请求超时".into()))?
            .map_err(redis_error)?;
            if acquired.is_none() {
                continue;
            }
            let requested_at = Instant::now();
            let remaining: i64 = tokio::time::timeout(
                REQUEST_TIMEOUT,
                redis::Script::new(CHECK_SCRIPT)
                    .key(&key)
                    .arg(&owner)
                    .invoke_async(&mut manager),
            )
            .await
            .map_err(|_| Error::RegisterSignFailed("节点所有权确认超时".into()))?
            .map_err(redis_error)?;
            if remaining <= 0 {
                return Err(Error::RegisterSignFailed(
                    "节点占用后未能确认有效所有权".into(),
                ));
            }
            let margin = Duration::from_millis((duration / 10).min(5_000));
            let state = Arc::new(LeaseState {
                valid_until: Mutex::new(requested_at),
                lost_owner: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
            });
            state.confirm(requested_at, remaining, margin)?;
            state.ensure_valid()?;
            let renewal_state = Arc::clone(&state);
            // 在构造返回前启动，不依赖宿主完成数据库、事件总线或其他后台服务初始化。
            let renewal = tokio::spawn(async move {
                let period = Duration::from_millis((duration / 3).min(30_000));
                let mut timer = tokio::time::interval(period);
                timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                timer.tick().await;
                loop {
                    timer.tick().await;
                    if renewal_state.stopped.load(Ordering::Acquire) {
                        return;
                    }
                    let requested_at = Instant::now();
                    let result = tokio::time::timeout(
                        REQUEST_TIMEOUT,
                        redis::Script::new(RENEW_SCRIPT)
                            .key(&key)
                            .arg(&owner)
                            .arg(duration)
                            .invoke_async::<i64>(&mut manager),
                    )
                    .await;
                    match result {
                        Ok(Ok(remaining)) if remaining > 0 => {
                            if renewal_state
                                .confirm(requested_at, remaining, margin)
                                .is_err()
                            {
                                renewal_state.lost_owner.store(true, Ordering::Release);
                                eprintln!("[identifier_generator] 无法确认节点有效期，停止发号");
                                return;
                            }
                        }
                        Ok(Ok(_)) => {
                            renewal_state.lost_owner.store(true, Ordering::Release);
                            eprintln!(
                                "[identifier_generator] 节点所有权丢失，停止发号，需要重启实例"
                            );
                            return;
                        }
                        Ok(Err(error)) => eprintln!(
                            "[identifier_generator] 节点续期失败（{:?}），不延长本地有效期",
                            error.kind()
                        ),
                        Err(_) => {
                            eprintln!("[identifier_generator] 节点续期超时，不延长本地有效期")
                        }
                    }
                }
            });
            return Ok((
                node,
                Self {
                    state,
                    renewal: tokio::sync::Mutex::new(Some(renewal)),
                },
            ));
        }
        Err(Error::RegisterSignFailed("1024 个雪花节点均已占用".into()))
    }

    pub(crate) fn ensure_valid(&self) -> Result<(), Error> {
        self.state.ensure_valid()
    }

    pub(crate) async fn shutdown(&self) {
        self.state.stopped.store(true, Ordering::Release);
        if let Some(task) = self.renewal.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        // 不删除 Redis 键，保留占用直到自然到期，避免节点立即被重新使用。
    }
}

impl Drop for RedisNodeLease {
    fn drop(&mut self) {
        self.state.stopped.store(true, Ordering::Release);
        if let Some(task) = self.renewal.get_mut().take() {
            task.abort();
        }
    }
}

fn redis_error(error: redis::RedisError) -> Error {
    // 不把连接密码或服务端任意响应带入日志。
    Error::RegisterSignFailed(format!("共享 ID Redis 操作失败: {:?}", error.kind()))
}
