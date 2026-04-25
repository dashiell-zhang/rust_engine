use async_trait::async_trait;
use dashmap::DashMap;
use distributed_lock::{DistributedLock, LockError, LockGuard};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

/// 锁分组，管理同一 key 下的所有信号量槽位
///
/// 每个 key 对应一个 `LockGroup`，组内按槽位序号维护独立的 `Semaphore`
/// 当 `semaphore = N` 时，组内会有 0..N-1 共 N 个槽位，各自独立互斥
struct LockGroup {
    /// 各槽位的信号量表，key 为槽位序号（从 0 开始），value 为该槽位的信号量
    ///
    /// 每个槽位的 `Semaphore` 初始许可数为 1，表示同一时刻只允许一个持有者占用该槽位
    /// 槽位在首次被访问时延迟创建，避免预分配过多未使用的槽位
    slots: DashMap<u32, Arc<Semaphore>>,

    /// 当前活跃的持有者数量
    ///
    /// 在 `try_acquire` 成功时递增，在 `LockGuard` drop 时递减
    /// 归零时表示此分组已无任何持有者，可以安全从全局表中移除
    active_handles: AtomicI32,
}

impl LockGroup {
    /// 创建新的锁分组，包裹在 Arc 中以支持跨任务共享
    fn new() -> Arc<Self> {
        Arc::new(Self {
            slots: DashMap::new(),
            active_handles: AtomicI32::new(0),
        })
    }

    /// 获取指定序号的信号量槽位，不存在时创建并插入
    ///
    /// 返回 `Arc<Semaphore>` 的克隆，调用方可独立持有而不影响分组结构
    fn get_or_create_slot(&self, index: u32) -> Arc<Semaphore> {
        self.slots
            .entry(index)
            .or_insert_with(|| Arc::new(Semaphore::new(1)))
            .clone()
    }
}

/// 启动过期自动释放任务
///
/// 在 tokio 上生成一个后台任务，同时等待两个信号：
/// - `expiry` 计时到达：取出 permit 使其 drop，归还信号量名额（锁过期）
/// - `cancel_rx` 收到信号或 sender 被 drop：正常退出，不释放 permit（主动续期或主动释放）
///
/// 使用 `tokio::select!` 保证两路信号互斥，先到者触发，另一路自动取消
fn spawn_expiry_task(
    shared: Arc<Mutex<Option<OwnedSemaphorePermit>>>,
    cancel_rx: oneshot::Receiver<()>,
    expiry: Duration,
) {
    tokio::spawn(async move {
        tokio::select! {

            // 到期分支：取出 permit 并 drop，信号量许可归还，槽位可被新的竞争者获取
            _ = tokio::time::sleep(expiry) => {
                if let Ok(mut guard) = shared.lock() {
                    guard.take();
                }
            }

            // 取消分支：sender 被 drop 或主动发送时触发，任务静默退出，permit 不变
            _ = cancel_rx => {}

        }
    });
}

/// 基于进程内内存的分布式锁实现
///
/// 使用 `tokio::sync::Semaphore` 作为槽位互斥原语，通过 `DashMap` 管理锁分组
/// 不支持跨进程与跨机器互斥，仅适用于单进程内的并发控制
///
/// # 线程安全
///
/// 内部所有共享状态均通过 `Arc` 包装，`InMemoryLock` 可安全地跨任务克隆或共享引用
pub struct InMemoryLock {
    /// 全局锁分组表，以原始 key 为维度管理所有槽位
    ///
    /// 使用 `DashMap` 实现分片并发，减少全局锁竞争
    /// 分组在首次调用 lock/try_lock 时按需创建，持有者全部释放后自动清理
    groups: Arc<DashMap<String, Arc<LockGroup>>>,
}

impl InMemoryLock {
    /// 创建新的内存锁实例
    pub fn new() -> Self {
        Self {
            groups: Arc::new(DashMap::new()),
        }
    }

    /// 对指定 key 尝试获取一次锁
    ///
    /// 遍历 0..semaphore 的所有槽位，对每个槽位调用 `try_acquire_owned`（非阻塞）
    /// 任意槽位成功即返回句柄；所有槽位均被占用则返回 `None`
    async fn try_acquire(&self, key: &str, expiry: Duration, semaphore: u32) -> Option<LockGuard> {
        // 获取或创建此 key 对应的分组，克隆 Arc 后立即释放 DashMap 的分片锁
        let group = self
            .groups
            .entry(key.to_string())
            .or_insert_with(LockGroup::new)
            .clone();

        for i in 0..semaphore {
            let slot = group.get_or_create_slot(i);

            // try_acquire_owned 返回 OwnedSemaphorePermit，与 Arc<Semaphore> 生命周期解耦
            // 成功时 permit drop 即归还许可；失败时此槽位已被占用，尝试下一个槽位
            if let Ok(permit) = Arc::clone(&slot).try_acquire_owned() {
                group.active_handles.fetch_add(1, Ordering::SeqCst);
                return Some(self.build_guard(key.to_string(), permit, expiry, Arc::clone(&group)));
            }
        }

        None
    }

    /// 构建锁句柄
    ///
    /// 将 permit 包装进共享容器，注入过期任务、释放回调与续期回调
    /// 调用方在获取 permit 后立即调用此方法，之后通过返回的 `LockGuard` 管理锁的生命周期
    fn build_guard(
        &self,
        key: String,
        permit: OwnedSemaphorePermit,
        expiry: Duration,
        group: Arc<LockGroup>,
    ) -> LockGuard {
        // permit 包装在 Arc<Mutex<Option<...>>> 中，使过期任务和 release_fn 都能安全地取出并释放它
        // 两者均通过 Option::take 操作，先到者取出 permit 使其 drop，后到者取出 None 为空操作
        let shared: Arc<Mutex<Option<OwnedSemaphorePermit>>> = Arc::new(Mutex::new(Some(permit)));

        // oneshot 通道用于取消当前过期任务
        // release_fn 和 local_renew_fn 在需要停止过期任务时，只需 drop sender 即可触发 cancel_rx
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        let cancel_state: Arc<Mutex<Option<oneshot::Sender<()>>>> =
            Arc::new(Mutex::new(Some(cancel_tx)));

        // 启动初始过期任务，expiry 到达时自动释放 permit
        spawn_expiry_task(Arc::clone(&shared), cancel_rx, expiry);

        // ── release_fn：在 LockGuard drop 时执行 ──────────────────────────────
        let shared_for_drop = Arc::clone(&shared);
        let cancel_for_drop = Arc::clone(&cancel_state);
        let group_for_drop = Arc::clone(&group);
        let groups_for_drop = Arc::clone(&self.groups);
        let key_for_drop = key.clone();

        let release_fn = move || {
            // 取消当前过期任务，避免任务在 permit 已释放后再次尝试 take（虽然是空操作，但节省资源）
            if let Ok(mut g) = cancel_for_drop.lock() {
                g.take();
            }

            // 释放信号量名额，permit drop 后对应槽位可被新的竞争者获取
            if let Ok(mut g) = shared_for_drop.lock() {
                g.take();
            }

            // 减少活跃持有者计数；fetch_sub 返回操作前的旧值，旧值为 1 表示当前是最后一个持有者
            // 归零后尝试移除分组，使用 ptr_eq 确保不误删同 key 下新创建的分组
            if group_for_drop.active_handles.fetch_sub(1, Ordering::SeqCst) == 1 {
                groups_for_drop.remove_if(&key_for_drop, |_, v| Arc::ptr_eq(v, &group_for_drop));
            }
        };

        // ── local_renew_fn：在 DistributedLock::renew 时执行 ─────────────────
        let shared_for_renew = Arc::clone(&shared);
        let cancel_for_renew = Arc::clone(&cancel_state);

        let local_renew_fn = move |new_expiry: Duration| -> bool {
            // 先取消旧的过期任务，再检查 permit 是否仍然有效
            // 顺序重要：若反过来先检查再取消，旧任务可能在检查通过后、取消前恰好释放 permit
            let old_tx = cancel_for_renew.lock().ok().and_then(|mut g| g.take());
            drop(old_tx);

            // permit 已被过期任务取走，说明锁已失效，续期失败
            if shared_for_renew.lock().map_or(true, |g| g.is_none()) {
                return false;
            }

            // 创建新的取消通道，并启动新的过期任务
            let (new_tx, new_rx) = oneshot::channel::<()>();

            if let Ok(mut g) = cancel_for_renew.lock() {
                *g = Some(new_tx);
            }

            spawn_expiry_task(Arc::clone(&shared_for_renew), new_rx, new_expiry);

            true
        };

        LockGuard::new(
            key,
            String::from("in-memory"),
            Box::new(release_fn),
            Some(Box::new(local_renew_fn)),
        )
    }
}

impl Default for InMemoryLock {
    /// 创建默认内存锁实例
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DistributedLock for InMemoryLock {
    /// 阻塞式获取内存锁
    ///
    /// 以 100ms 为间隔轮询，直到获取成功或超过 `expiry` 超时
    /// `expiry` 同时作为锁的失效时长（从获取成功时刻起算）和等待获取的最长时间
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
        let deadline = tokio::time::Instant::now() + expiry;

        loop {
            if let Some(guard) = self.try_acquire(key, expiry, semaphore).await {
                return Ok(guard);
            }

            // 超过截止时间则放弃，返回超时错误
            if tokio::time::Instant::now() >= deadline {
                return Err(LockError::Timeout {
                    key: key.to_string(),
                });
            }

            // 等待 100ms 再重试，与 NetEngine 保持一致
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// 非阻塞式尝试获取内存锁
    ///
    /// 立即尝试一次，所有槽位均被占用时返回 `None`，不进行重试
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

        Ok(self.try_acquire(key, expiry, semaphore).await)
    }

    /// 续期内存锁
    ///
    /// 通过 `LockGuard` 内部的 `local_renew` 回调实现：
    /// 取消当前过期任务，以新的 `new_expiry` 重新启动
    /// 若 permit 已被过期任务释放（锁已失效），返回 `false`
    async fn renew(&self, handle: &LockGuard, expiry: Duration) -> Result<bool, LockError> {
        let expiry = if expiry.is_zero() {
            Duration::from_secs(60)
        } else {
            expiry
        };

        match &handle.local_renew {
            Some(renew_fn) => Ok(renew_fn(expiry)),
            // local_renew 为 None 表示此句柄来自非内存锁实现，不支持本方法续期
            None => Ok(false),
        }
    }
}
