use async_trait::async_trait;
use distributed_lock::{DistributedLock, LockError, LockGuard};
use md5::{Digest, Md5};
use redis::{AsyncCommands, aio::ConnectionManager};
use std::time::Duration;

/// Redis 锁的标识值
///
/// 所有锁操作使用相同的固定值，通过 Redis 的 SET NX 原子性保证同一 key 同时只有一个持有者
/// 续期 Lua 脚本通过比对此值来确认操作方是当前持有者
const LOCK_VALUE: &str = "1";

/// Redis 锁续期用的 Lua 脚本
///
/// 先通过 GET 校验 key 的当前值与 ARGV[1] 是否一致，一致则执行 PEXPIRE 更新过期时间
/// 使用 Lua 保证校验与续期的原子性，避免 TOCTOU 竞争
///
/// KEYS[1]：锁的 Redis key
/// ARGV[1]：期望的标识值（必须与持有者写入的值一致才续期）
/// ARGV[2]：新的过期时长（毫秒）
/// 返回 1 表示续期成功，0 表示 key 不存在或 value 不匹配
const RENEW_SCRIPT: &str = r#"
    if redis.call('get', KEYS[1]) == ARGV[1] then
        return redis.call('pexpire', KEYS[1], ARGV[2])
    else
        return 0
    end
"#;

/// 基于 Redis 的分布式锁实现
///
/// 使用 `SET key value NX PX ms` 原子命令获取锁，支持跨进程与跨机器互斥
/// 内部持有 `ConnectionManager`，支持自动重连，可安全地克隆并在多个任务间共享引用
pub struct RedisLock {
    /// Redis 连接管理器
    ///
    /// `ConnectionManager` 内部维护单一连接并在断线时自动重连
    /// 克隆操作为 Arc 克隆，开销极低，多个任务可共享同一底层连接
    conn_mgr: ConnectionManager,

    /// Key 前缀，用于隔离不同服务或实例的锁命名空间
    ///
    /// 最终写入 Redis 的 key 格式为 `{prefix}{md5(key)} {slot}`
    prefix: String,
}

impl RedisLock {
    /// 通过连接字符串创建 Redis 锁实例
    ///
    /// 会立即尝试建立 Redis 连接，连接失败时返回 `LockError::InternalError`
    ///
    /// - `connection_str`：Redis 连接字符串，格式如 `redis://127.0.0.1/`
    /// - `prefix`：Key 前缀，建议使用服务名称或实例标识，避免不同服务的锁 key 冲突
    pub async fn new(connection_str: &str, prefix: &str) -> Result<Self, LockError> {
        let client = redis::Client::open(connection_str)
            .map_err(|error| LockError::InternalError(error.to_string()))?;
        let conn_mgr = ConnectionManager::new(client)
            .await
            .map_err(|error| LockError::InternalError(error.to_string()))?;

        Ok(Self {
            conn_mgr,
            prefix: prefix.to_string(),
        })
    }

    /// 构造写入 Redis 的完整 key
    ///
    /// 对齐 NetEngine 的 Redis 锁 key 规则：
    /// 先对原始 key 做 MD5，再拼接前缀和信号量槽位
    /// 最终格式为 `{prefix}{md5(key)} {slot}`
    fn make_key(&self, key: &str, slot: u32) -> String {
        let key_hash = Md5::digest(key.as_bytes());
        let key_hash = key_hash
            .iter()
            .map(|value| format!("{value:02X}"))
            .collect::<String>();
        format!("{}{} {}", self.prefix, key_hash, slot)
    }

    /// 尝试通过 SET NX PX 原子命令占用一个 Redis 槽位
    ///
    /// SET key value NX PX ms：
    /// - NX：key 不存在时才写入，保证互斥
    /// - PX：写入时同时设置过期时间（毫秒），防止持有者崩溃后锁永久泄漏
    ///
    /// 返回 `true` 表示写入成功（锁已获取），`false` 表示 key 已存在或命令执行失败
    async fn try_acquire_slot(
        conn: &mut ConnectionManager,
        redis_key: &str,
        value: &str,
        expiry_ms: u64,
    ) -> bool {
        // Redis 在 SET NX 成功时返回 "+OK"，失败时返回 nil（即 None）
        let result: Option<String> = redis::cmd("SET")
            .arg(redis_key)
            .arg(value)
            .arg("NX")
            .arg("PX")
            .arg(expiry_ms)
            .query_async(conn)
            .await
            .unwrap_or(None);

        result.is_some()
    }

    /// 构建 Redis 锁句柄
    ///
    /// 注入 fire-and-forget 风格的释放回调：drop 时在后台任务中发送 DEL 命令
    /// 不等待 DEL 结果，以避免在 `Drop` 中阻塞当前任务
    ///
    /// Redis 锁的续期由 `RedisLock::renew` 通过连接执行，句柄本身不持有续期回调
    fn build_guard(&self, redis_key: String, value: String) -> LockGuard {
        // 克隆 ConnectionManager（Arc 克隆，开销极低），供释放回调捕获
        let conn = self.conn_mgr.clone();
        let key_for_drop = redis_key.clone();

        let release_fn = move || {
            let mut c = conn.clone();
            let k = key_for_drop.clone();

            // 在 tokio 后台任务中执行 DEL，不阻塞当前线程
            // 若持有者正常释放（未超期），DEL 成功删除 key，槽位立即可被新竞争者获取
            // 若锁已超期（key 已被 Redis 自动删除），DEL 对不存在的 key 为空操作
            tokio::spawn(async move {
                let _: Result<i64, _> = c.del(&k).await;
            });
        };

        LockGuard::new(redis_key, value, Box::new(release_fn), None)
    }
}

#[async_trait]
impl DistributedLock for RedisLock {
    /// 阻塞式获取 Redis 锁
    ///
    /// 以 100ms 为间隔轮询所有槽位，直到任意槽位被成功写入或超过 `expiry` 超时
    /// 每次轮询对所有槽位各尝试一次 SET NX，获取任意一个即返回
    async fn lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<LockGuard, LockError> {
        if key.is_empty() {
            return Err(LockError::InvalidArgument {
                message: "key 不能为空".into(),
            });
        }

        if semaphore == 0 {
            return Err(LockError::InvalidArgument {
                message: "semaphore 必须大于 0".into(),
            });
        }

        let expiry = if expiry.is_zero() {
            Duration::from_secs(60)
        } else {
            expiry
        };
        // as_millis 返回 u128，截断为 u64；实际使用中不会传入超过 584 亿年的超时值
        let expiry_ms = expiry.as_millis() as u64;
        let deadline = tokio::time::Instant::now() + expiry;

        loop {
            // 每轮克隆一次连接（Arc 克隆，开销极低），保持各轮独立
            let mut conn = self.conn_mgr.clone();

            for i in 0..semaphore {
                let redis_key = self.make_key(key, i);

                if Self::try_acquire_slot(&mut conn, &redis_key, LOCK_VALUE, expiry_ms).await {
                    return Ok(self.build_guard(redis_key, LOCK_VALUE.to_string()));
                }
            }

            // 本轮所有槽位均被占用，检查是否已超过截止时间
            if tokio::time::Instant::now() >= deadline {
                return Err(LockError::Timeout {
                    key: key.to_string(),
                });
            }

            // 等待 100ms 再重试，与 NetEngine 保持一致
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// 非阻塞式尝试获取 Redis 锁
    ///
    /// 立即遍历所有槽位各尝试一次 SET NX，获取任意一个即返回 `Some`
    /// 所有槽位均被占用时返回 `None`，不进行重试
    async fn try_lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<Option<LockGuard>, LockError> {
        if key.is_empty() {
            return Err(LockError::InvalidArgument {
                message: "key 不能为空".into(),
            });
        }

        if semaphore == 0 {
            return Err(LockError::InvalidArgument {
                message: "semaphore 必须大于 0".into(),
            });
        }

        let expiry = if expiry.is_zero() {
            Duration::from_secs(60)
        } else {
            expiry
        };
        let expiry_ms = expiry.as_millis() as u64;
        let mut conn = self.conn_mgr.clone();

        for i in 0..semaphore {
            let redis_key = self.make_key(key, i);

            if Self::try_acquire_slot(&mut conn, &redis_key, LOCK_VALUE, expiry_ms).await {
                return Ok(Some(self.build_guard(redis_key, LOCK_VALUE.to_string())));
            }
        }

        Ok(None)
    }

    /// 续期 Redis 锁
    ///
    /// 执行 Lua 脚本：校验 `lock_key` 对应的值与 `lock_value` 是否一致
    /// 一致则执行 PEXPIRE 将过期时间更新为新的 `expiry`，并返回 `true`
    /// key 不存在（已过期）或 value 不匹配时，Lua 脚本返回 0，此方法返回 `false`
    ///
    /// Lua 脚本保证校验与续期的原子性，无需担心 TOCTOU 竞争
    async fn renew(&self, handle: &LockGuard, expiry: Duration) -> Result<bool, LockError> {
        let expiry = if expiry.is_zero() {
            Duration::from_secs(60)
        } else {
            expiry
        };
        let expiry_ms = expiry.as_millis() as u64;
        let mut conn = self.conn_mgr.clone();

        // invoke_async 在 Redis 中以 EVAL 方式原子执行脚本
        // 脚本执行异常或网络错误时 unwrap_or(0) 视为续期失败
        let result: i64 = redis::Script::new(RENEW_SCRIPT)
            .key(&handle.lock_key)
            .arg(&handle.lock_value)
            .arg(expiry_ms)
            .invoke_async(&mut conn)
            .await
            .unwrap_or(0);

        Ok(result == 1)
    }
}
