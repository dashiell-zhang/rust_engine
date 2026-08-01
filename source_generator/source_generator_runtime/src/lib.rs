use std::time::Duration;

use distributed_lock::DistributedLock;
use hybrid_cache::HybridCache;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::{Display, Formatter};
use std::sync::Arc;

/// 重新导出分布式锁错误，供属性宏在调用方保留底层错误类型
pub use distributed_lock::LockError;

/// 参数键生成失败
#[derive(Debug)]
pub struct InvocationKeyError {
    message: String,
}

impl InvocationKeyError {
    /// 创建参数键错误
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for InvocationKeyError {
    /// 输出参数键错误描述
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for InvocationKeyError {}

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
    uuid::Uuid::now_v7().to_string()
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
    let duration_ms = u64::try_from(duration_ms).unwrap_or(u64::MAX);
    tracing::info!(trace_id, method, duration_ms, event = "executed",);
}

/// 记录方法调用异常日志
///
/// 在方法体返回 Err 时输出，包含追踪 ID、方法路径、执行耗时与错误描述，event 固定为 exception
pub fn log_exception(method: &str, trace_id: &str, duration_ms: u128, error: &str) {
    let duration_ms = u64::try_from(duration_ms).unwrap_or(u64::MAX);
    tracing::error!(trace_id, method, duration_ms, error, event = "exception",);
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

/// 记录即将执行下一次重试
pub fn log_retry_scheduled(method: &str, retry: u64, delay_ms: u64) {
    tracing::warn!(method, retry, delay_ms, event = "retry_scheduled");
}

/// 记录重试次数已经耗尽
pub fn log_retry_exhausted(method: &str, retries: u64) {
    tracing::warn!(method, retries, event = "retry_exhausted");
}

/// 记录缓存读取、写入或防击穿锁故障并继续执行业务
pub fn log_cache_degraded(method: &str, operation: &str, error: &dyn Display) {
    tracing::warn!(method, operation, error = %error, event = "cache_degraded");
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

/// 生成包含模块、实现类型、方法名和参数类型的稳定方法标识
pub fn method_key(module: &str, self_type: Option<&str>, fn_name: &str, signature: &str) -> String {
    match self_type {
        Some(self_type) => format!("{module}|{self_type}::{fn_name}{signature}"),
        None => format!("{module}|{fn_name}{signature}"),
    }
}

/// 将调用参数序列化为有边界的规范 JSON
pub fn arguments_key(args: &impl Serialize) -> Result<String, InvocationKeyError> {
    const MAX_DEPTH: usize = 64;
    const MAX_ITEMS: usize = 10_000;
    const MAX_BYTES: usize = 1024 * 1024;

    let value = serde_json::to_value(args)
        .map_err(|error| InvocationKeyError::new(format!("参数无法序列化为 JSON: {error}")))?;
    let mut item_count = 0usize;
    let value = canonicalize_json(value, 0, &mut item_count, MAX_DEPTH, MAX_ITEMS)?;
    let json = serde_json::to_string(&value)
        .map_err(|error| InvocationKeyError::new(format!("参数 JSON 无法编码: {error}")))?;

    if json.len() > MAX_BYTES {
        return Err(InvocationKeyError::new(format!(
            "参数 JSON 超过 {MAX_BYTES} 字节限制"
        )));
    }

    Ok(json)
}

/// 生成 cacheable 行为使用的缓存键
pub fn cacheable_key(method_key: &str, arguments_key: &str) -> String {
    format!(
        "CacheData_{}",
        compose_invocation_hash(method_key, Some(arguments_key))
    )
}

/// 生成 cacheable 行为使用的防击穿锁键
pub fn cacheable_lock_key(method_key: &str, arguments_key: &str) -> String {
    format!(
        "CacheDataLock_{}",
        compose_invocation_hash(method_key, Some(arguments_key))
    )
}

/// 生成 concurrency_limit 行为使用的锁键
pub fn concurrency_limit_key(method_key: &str, arguments_key: Option<&str>) -> String {
    format!(
        "ConcurrencyLimit_{}",
        compose_invocation_hash(method_key, arguments_key)
    )
}

/// 递归规范化 JSON 对象字段顺序并限制输入复杂度
fn canonicalize_json(
    value: serde_json::Value,
    depth: usize,
    item_count: &mut usize,
    max_depth: usize,
    max_items: usize,
) -> Result<serde_json::Value, InvocationKeyError> {
    if depth > max_depth {
        return Err(InvocationKeyError::new(format!(
            "参数 JSON 超过 {max_depth} 层深度限制"
        )));
    }

    *item_count = item_count
        .checked_add(1)
        .ok_or_else(|| InvocationKeyError::new("参数 JSON 项目计数溢出"))?;

    if *item_count > max_items {
        return Err(InvocationKeyError::new(format!(
            "参数 JSON 超过 {max_items} 项限制"
        )));
    }

    match value {
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(|value| canonicalize_json(value, depth + 1, item_count, max_depth, max_items))
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        serde_json::Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));

            entries
                .into_iter()
                .map(|(key, value)| {
                    canonicalize_json(value, depth + 1, item_count, max_depth, max_items)
                        .map(|value| (key, value))
                })
                .collect::<Result<serde_json::Map<_, _>, _>>()
                .map(serde_json::Value::Object)
        }
        value => Ok(value),
    }
}

/// 计算方法标识与可选参数快照组成的调用摘要
fn compose_invocation_hash(method_key: &str, arguments_key: Option<&str>) -> String {
    let mut seed = method_key.to_string();

    if let Some(arguments_key) = arguments_key {
        seed.push('\n');
        seed.push_str(arguments_key);
    }

    sha256_hex(&seed)
}

/// 计算输入字符串的 SHA-256 十六进制摘要
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();

    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}
