use std::time::Duration;

use distributed_lock::DistributedLock;
use hybrid_cache::{HybridCache, HybridCacheError};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// 重试行为选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryOptions {
    /// 最大重试次数
    pub max_retries: u32,

    /// 每次重试前的等待秒数
    pub delay_seconds: u64,
}

impl Default for RetryOptions {
    /// 返回与 NetEngine 对齐的默认重试配置
    fn default() -> Self {
        Self {
            max_retries: 3,
            delay_seconds: 0,
        }
    }
}

/// 缓存行为选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheableOptions {
    /// 缓存有效期秒数
    pub ttl_seconds: u64,
}

impl Default for CacheableOptions {
    /// 返回与 NetEngine 对齐的默认缓存配置
    fn default() -> Self {
        Self { ttl_seconds: 60 }
    }
}

/// 并发限制行为选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConcurrencyLimitOptions {
    /// 是否将参数加入锁键
    pub is_use_parameter: bool,

    /// 获取不到锁时是否直接返回错误
    pub is_block: bool,

    /// 锁过期秒数
    pub expiry_seconds: u64,

    /// 允许的最大并发数
    pub semaphore: u32,
}

impl Default for ConcurrencyLimitOptions {
    /// 返回与 NetEngine 对齐的默认并发限制配置
    fn default() -> Self {
        Self {
            is_use_parameter: false,
            is_block: false,
            expiry_seconds: 0,
            semaphore: 1,
        }
    }
}

/// 为 async fn 方法自动注入结构化调用日志的属性宏
///
/// 详见 source_generator_core::logging
pub use source_generator_core::logging;

/// 为 async fn 方法开启自动重试的属性宏
///
/// 适合参数为 `Copy` 类型或引用的方法
/// 若方法体在 `.await` 点跨越移动了 owned 参数，编译器会报错，此时应改为传引用或在调用侧处理重试
///
/// 详见 source_generator_core::retry
pub use source_generator_core::retry;

/// 为 async fn 方法开启缓存包装的属性宏
///
/// 详见 source_generator_core::cacheable
pub use source_generator_core::cacheable;

/// 为 async fn 方法开启并发限制包装的属性宏
///
/// 详见 source_generator_core::concurrency_limit
pub use source_generator_core::concurrency_limit;

/// 生成应用服务工厂方法的函数宏
///
/// 详见 source_generator_core::generate_service_factory
pub use source_generator_core::generate_service_factory;

/// 生成直接服务工厂方法的函数宏
///
/// 详见 source_generator_core::generate_direct_service_factory
pub use source_generator_core::generate_direct_service_factory;

/// 生成任务注册表批量注册语句的函数宏
///
/// 详见 source_generator_core::register_task_registry
pub use source_generator_core::register_task_registry;

/// 生成新的调用追踪 ID
///
/// 每次方法调用时生成唯一标识，用于在日志中关联同一次调用的 executing/executed/exception 三条记录
pub fn new_trace_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 记录方法调用开始日志
///
/// 在方法体执行前输出，包含追踪 ID 与方法路径，event 固定为 executing
pub fn log_executing(method: &str, trace_id: &str) {
    tracing::info!(trace_id, method, event = "executing",);
}

/// 记录方法调用完成日志
///
/// 在方法体正常返回后输出，包含追踪 ID、方法路径与执行耗时，event 固定为 executed
pub fn log_executed(method: &str, trace_id: &str, duration_ms: u128) {
    // duration_ms 由 u128 强转为 u64，tracing 字段不支持 u128，实际耗时不会溢出 u64
    tracing::info!(
        trace_id,
        method,
        duration_ms = duration_ms as u64,
        event = "executed",
    );
}

/// 记录方法调用异常日志
///
/// 在方法体返回 Err 时输出，包含追踪 ID、方法路径、执行耗时与错误描述，event 固定为 exception
pub fn log_exception(method: &str, trace_id: &str, duration_ms: u128, error: &str) {
    // duration_ms 由 u128 强转为 u64，tracing 字段不支持 u128，实际耗时不会溢出 u64
    tracing::error!(
        trace_id,
        method,
        duration_ms = duration_ms as u64,
        error,
        event = "exception",
    );
}

/// 重试间隔等待
///
/// delay_ms 为 0 时直接返回，不产生任何调度开销
pub async fn retry_sleep(delay_ms: u64) {
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

/// 将秒级重试等待时间转换为毫秒
pub fn retry_delay_ms_from_seconds(delay_seconds: u64) -> u64 {
    delay_seconds.saturating_mul(1000)
}

/// 归一化最大重试次数
pub fn normalize_retry_max_retries(max_retries: u32) -> u32 {
    max_retries
}

/// 归一化并发限制信号量数量
pub fn normalize_concurrency_limit_semaphore(semaphore: u32) -> u32 {
    semaphore.max(1)
}

/// 混合缓存引用适配接口
pub trait AsHybridCacheRef {
    /// 获取混合缓存引用
    fn as_hybrid_cache_ref(&self) -> &HybridCache;
}

impl AsHybridCacheRef for HybridCache {
    /// 返回混合缓存自身引用
    fn as_hybrid_cache_ref(&self) -> &HybridCache {
        self
    }
}

impl AsHybridCacheRef for Arc<HybridCache> {
    /// 返回 Arc 包装的混合缓存引用
    fn as_hybrid_cache_ref(&self) -> &HybridCache {
        self.as_ref()
    }
}

impl<T> AsHybridCacheRef for &T
where
    T: AsHybridCacheRef + ?Sized,
{
    /// 透传引用包装的混合缓存引用
    fn as_hybrid_cache_ref(&self) -> &HybridCache {
        (*self).as_hybrid_cache_ref()
    }
}

/// 分布式锁引用适配接口
pub trait AsDistributedLockRef {
    /// 获取分布式锁引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock;
}

impl AsDistributedLockRef for dyn DistributedLock {
    /// 返回分布式锁自身引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock {
        self
    }
}

impl AsDistributedLockRef for Arc<dyn DistributedLock> {
    /// 返回 Arc 包装的分布式锁引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock {
        self.as_ref()
    }
}

impl AsDistributedLockRef for &Arc<dyn DistributedLock> {
    /// 返回引用包装的 Arc 分布式锁引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock {
        &***self
    }
}

impl AsDistributedLockRef for &(dyn DistributedLock + '_) {
    /// 返回引用包装的分布式锁引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock {
        *self
    }
}

/// 获取混合缓存引用
pub fn hybrid_cache_ref<T>(cache: &T) -> &HybridCache
where
    T: AsHybridCacheRef + ?Sized,
{
    cache.as_hybrid_cache_ref()
}

/// 获取分布式锁引用
pub fn distributed_lock_ref<T>(lock: &T) -> &dyn DistributedLock
where
    T: AsDistributedLockRef + ?Sized,
{
    lock.as_distributed_lock_ref()
}

/// 生成 cacheable 行为使用的缓存键
///
/// 键格式与 NetEngine 对齐，为 CacheData_ + SHA-256(method + serde_json(args))
pub fn cacheable_key(module: &str, fn_name: &str, args: &impl Serialize) -> String {
    let method = format!("{module}::{fn_name}");
    let seed = format!("{method}{}", serialize_args(args));
    format!("CacheData_{}", sha256_hex(&seed))
}

/// 生成 concurrency_limit 行为使用的锁键
///
/// 键格式与 NetEngine 对齐，为 ConcurrencyLimit_ + method[_serde_json(args)]
pub fn concurrency_limit_key(
    module: &str,
    fn_name: &str,
    is_use_parameter: bool,
    args: &impl Serialize,
) -> String {
    let mut seed = format!("ConcurrencyLimit_{module}::{fn_name}");

    if is_use_parameter {
        seed.push('_');
        seed.push_str(&serialize_args(args));
    }

    seed
}

/// 将原始方法异常转换为混合缓存工厂异常
pub fn hybrid_cache_factory_error<E>(error: E) -> HybridCacheError
where
    E: ToString,
{
    HybridCacheError::Factory(error.to_string())
}

/// 将参数序列化为稳定 JSON 字符串，用于缓存键与锁键计算
fn serialize_args(args: &impl Serialize) -> String {
    serde_json::to_string(args).unwrap_or_else(|_| "null".to_string())
}

/// 计算输入字符串的 SHA-256 十六进制摘要
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();

    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}
