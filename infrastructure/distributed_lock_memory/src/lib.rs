use async_trait::async_trait;
use dashmap::DashMap;
use distributed_lock::{DistributedLock, LockError, LockGuard};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

/// 等待锁时的最大轮询间隔
const RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// 长租期到期检查的单次最大等待时长
const EXPIRATION_DELAY_SEGMENT: Duration = Duration::from_secs(24 * 60 * 60);

/// 锁分组生命周期状态
struct LockGroupLifecycle {
    /// 正在获取或持有当前分组的引用数量
    reference_count: usize,

    /// 当前分组是否已从全局表移除
    removed: bool,
}

/// 锁分组，管理同一 key 下的所有信号量槽位
struct LockGroup {
    /// 各槽位的信号量表
    slots: DashMap<u32, Arc<Semaphore>>,

    /// 分组生命周期状态
    lifecycle: Mutex<LockGroupLifecycle>,
}

impl LockGroup {
    /// 创建新的锁分组
    fn new() -> Arc<Self> {
        Arc::new(Self {
            slots: DashMap::new(),
            lifecycle: Mutex::new(LockGroupLifecycle {
                reference_count: 0,
                removed: false,
            }),
        })
    }

    /// 获取指定序号的信号量槽位
    fn get_or_create_slot(&self, index: u32) -> Arc<Semaphore> {
        self.slots
            .entry(index)
            .or_insert_with(|| Arc::new(Semaphore::new(1)))
            .clone()
    }
}

/// 锁分组引用，在获取流程提前结束或被取消时自动归还引用
struct LockGroupReference {
    /// 锁名称
    key: String,

    /// 锁分组
    group: Arc<LockGroup>,

    /// 全局锁分组表
    groups: Arc<DashMap<String, Arc<LockGroup>>>,

    /// 当前引用是否仍由本对象负责释放
    active: bool,
}

impl LockGroupReference {
    /// 将分组引用转交给锁句柄
    fn transfer(&mut self) {
        self.active = false;
    }
}

impl Drop for LockGroupReference {
    /// 获取流程退出时归还尚未转交的分组引用
    fn drop(&mut self) {
        if self.active {
            release_group(&self.groups, &self.key, &self.group);
        }
    }
}

/// 内存锁句柄状态
struct InMemoryHandleState {
    /// 当前占用的信号量许可
    permit: Option<OwnedSemaphorePermit>,

    /// 当前到期任务代次
    generation: u64,

    /// 当前到期任务取消发送端
    expiry_cancel: Option<oneshot::Sender<()>>,

    /// 锁是否已经释放
    released: bool,
}

/// 内存锁句柄共享状态
struct InMemoryHandle {
    /// 锁名称
    key: String,

    /// 锁分组
    group: Arc<LockGroup>,

    /// 全局锁分组表
    groups: Arc<DashMap<String, Arc<LockGroup>>>,

    /// 句柄并发状态
    state: Mutex<InMemoryHandleState>,
}

impl InMemoryHandle {
    /// 创建内存锁句柄状态
    fn new(
        key: String,
        group: Arc<LockGroup>,
        groups: Arc<DashMap<String, Arc<LockGroup>>>,
        permit: OwnedSemaphorePermit,
    ) -> Arc<Self> {
        Arc::new(Self {
            key,
            group,
            groups,
            state: Mutex::new(InMemoryHandleState {
                permit: Some(permit),
                generation: 0,
                expiry_cancel: None,
                released: false,
            }),
        })
    }

    /// 安排新的到期释放任务
    fn renew(self: &Arc<Self>, expiry: Duration) -> bool {
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let (generation, previous_cancel) = {
            let mut state = lock_unpoisoned(&self.state);
            if state.released || state.permit.is_none() {
                return false;
            }

            state.generation = state.generation.wrapping_add(1);
            let generation = state.generation;
            let previous_cancel = state.expiry_cancel.replace(cancel_tx);
            (generation, previous_cancel)
        };

        drop(previous_cancel);
        spawn_expiry_task(Arc::clone(self), generation, cancel_rx, expiry);
        true
    }

    /// 主动释放锁
    fn release(&self) {
        let resources = {
            let mut state = lock_unpoisoned(&self.state);
            if state.released {
                return;
            }

            state.released = true;
            state.generation = state.generation.wrapping_add(1);
            (state.expiry_cancel.take(), state.permit.take())
        };

        drop(resources);
        release_group(&self.groups, &self.key, &self.group);
    }

    /// 在指定代次到期时释放锁
    fn expire(&self, generation: u64) {
        let permit = {
            let mut state = lock_unpoisoned(&self.state);
            if state.released || state.generation != generation {
                return;
            }

            state.released = true;
            state.expiry_cancel = None;
            state.permit.take()
        };

        drop(permit);
        release_group(&self.groups, &self.key, &self.group);
    }
}

/// 启动到期自动释放任务
fn spawn_expiry_task(
    handle: Arc<InMemoryHandle>,
    generation: u64,
    cancel_rx: oneshot::Receiver<()>,
    expiry: Duration,
) {
    tokio::spawn(async move {
        tokio::select! {
            _ = wait_for_expiry(expiry) => handle.expire(generation),
            _ = cancel_rx => {}
        }
    });
}

/// 以分段等待方式等待锁到期
async fn wait_for_expiry(expiry: Duration) {
    let start = Instant::now();

    loop {
        let remaining = expiry.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return;
        }

        tokio::time::sleep(remaining.min(EXPIRATION_DELAY_SEGMENT)).await;
    }
}

/// 基于进程内内存的分布式锁实现
///
/// 使用 `tokio::sync::Semaphore` 作为槽位互斥原语，通过 `DashMap` 管理锁分组
/// 不支持跨进程与跨机器互斥，仅适用于单进程内的并发控制
pub struct InMemoryLock {
    /// 全局锁分组表
    groups: Arc<DashMap<String, Arc<LockGroup>>>,
}

impl InMemoryLock {
    /// 创建新的内存锁实例
    pub fn new() -> Self {
        Self {
            groups: Arc::new(DashMap::new()),
        }
    }

    /// 获取并保留指定名称的锁分组
    fn acquire_group(&self, key: &str) -> LockGroupReference {
        loop {
            let group = self
                .groups
                .entry(key.to_string())
                .or_insert_with(LockGroup::new)
                .clone();

            let mut lifecycle = lock_unpoisoned(&group.lifecycle);
            if lifecycle.removed {
                continue;
            }

            lifecycle.reference_count += 1;
            drop(lifecycle);

            return LockGroupReference {
                key: key.to_string(),
                group,
                groups: Arc::clone(&self.groups),
                active: true,
            };
        }
    }

    /// 尝试从指定分组获取一个信号量槽位
    fn try_acquire_slot(group: &Arc<LockGroup>, semaphore: u32) -> Option<OwnedSemaphorePermit> {
        for index in 0..semaphore {
            let slot = group.get_or_create_slot(index);
            if let Ok(permit) = slot.try_acquire_owned() {
                return Some(permit);
            }
        }

        None
    }

    /// 构建锁句柄并转交锁分组引用
    fn build_guard(
        &self,
        key: String,
        mut group_reference: LockGroupReference,
        permit: OwnedSemaphorePermit,
        expiry: Duration,
    ) -> LockGuard {
        let handle = InMemoryHandle::new(
            key.clone(),
            Arc::clone(&group_reference.group),
            Arc::clone(&self.groups),
            permit,
        );
        group_reference.transfer();

        let _ = handle.renew(expiry);

        let handle_for_release = Arc::clone(&handle);
        let release = move || handle_for_release.release();

        let handle_for_renew = Arc::clone(&handle);
        let renew = move |new_expiry: Duration| handle_for_renew.renew(new_expiry);

        LockGuard::new(
            key,
            String::from("in-memory"),
            Box::new(release),
            Some(Box::new(renew)),
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
    async fn lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<LockGuard, LockError> {
        validate_arguments(key, semaphore)?;
        let expiry = normalize_expiry(expiry);
        let start = Instant::now();
        let group_reference = self.acquire_group(key);

        loop {
            if let Some(permit) = Self::try_acquire_slot(&group_reference.group, semaphore) {
                return Ok(self.build_guard(key.to_string(), group_reference, permit, expiry));
            }

            let remaining = expiry.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(LockError::Timeout {
                    key: key.to_string(),
                });
            }

            tokio::time::sleep(remaining.min(RETRY_INTERVAL)).await;
        }
    }

    /// 非阻塞式尝试获取内存锁
    async fn try_lock(
        &self,
        key: &str,
        expiry: Duration,
        semaphore: u32,
    ) -> Result<Option<LockGuard>, LockError> {
        validate_arguments(key, semaphore)?;
        let expiry = normalize_expiry(expiry);
        let group_reference = self.acquire_group(key);

        let Some(permit) = Self::try_acquire_slot(&group_reference.group, semaphore) else {
            return Ok(None);
        };

        Ok(Some(self.build_guard(
            key.to_string(),
            group_reference,
            permit,
            expiry,
        )))
    }

    /// 续期内存锁
    async fn renew(&self, handle: &LockGuard, expiry: Duration) -> Result<bool, LockError> {
        let expiry = normalize_expiry(expiry);

        match &handle.local_renew {
            Some(renew) => Ok(renew(expiry)),
            None => Ok(false),
        }
    }
}

/// 释放锁分组引用并在无人使用时移除分组
fn release_group(groups: &DashMap<String, Arc<LockGroup>>, key: &str, group: &Arc<LockGroup>) {
    let mut lifecycle = lock_unpoisoned(&group.lifecycle);
    if lifecycle.reference_count == 0 {
        return;
    }

    lifecycle.reference_count -= 1;
    if lifecycle.reference_count != 0 {
        return;
    }

    lifecycle.removed = true;
    groups.remove_if(key, |_, current| Arc::ptr_eq(current, group));
}

/// 获取忽略中毒标记的互斥锁守卫
fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// 规范化锁失效时长
fn normalize_expiry(expiry: Duration) -> Duration {
    if expiry.is_zero() {
        Duration::from_secs(60)
    } else {
        expiry
    }
}

/// 校验锁名称和信号量参数
fn validate_arguments(key: &str, semaphore: u32) -> Result<(), LockError> {
    if key.trim().is_empty() {
        return Err(LockError::InvalidArgument {
            message: "key 不能为空".into(),
        });
    }

    if semaphore == 0 {
        return Err(LockError::InvalidArgument {
            message: "semaphore 必须大于 0".into(),
        });
    }

    Ok(())
}
