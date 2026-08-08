use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, LinkedList, VecDeque};
use std::hash::BuildHasher;
use std::rc::Rc;
use std::time::Duration;

use distributed_lock::DistributedLock;
use hybrid_cache::{HybridCache, HybridCacheError};
use serde::{
    de::DeserializeOwned,
    ser::{
        Error as SerializeError, SerializeMap, SerializeSeq, SerializeStruct,
        SerializeStructVariant, SerializeTuple, SerializeTupleStruct, SerializeTupleVariant,
        Serializer,
    },
};
use sha2::{Digest, Sha256};
use std::fmt::{self, Debug, Display, Formatter};
use std::io::{self, Write};
use std::sync::Arc;

/// 重新导出分布式锁错误，供属性宏在调用方保留底层错误类型
pub use distributed_lock::LockError;

/// 重新导出参数键序列化约束，供属性宏生成针对性编译诊断
#[doc(hidden)]
pub use serde::Serialize;

/// 缓存防击穿锁的默认有效期秒数
pub const DEFAULT_CACHEABLE_LOCK_EXPIRY_SECONDS: u64 = 60;

/// 并发限制默认不使用参数生成锁键
pub const DEFAULT_CONCURRENCY_LIMIT_USE_PARAMETER: bool = false;

/// 并发限制默认等待获取锁
pub const DEFAULT_CONCURRENCY_LIMIT_IS_BLOCK: bool = false;

/// 并发限制默认锁过期秒数
pub const DEFAULT_CONCURRENCY_LIMIT_EXPIRY_SECONDS: u64 = 0;

/// 并发限制默认信号量数量
pub const DEFAULT_CONCURRENCY_LIMIT_SEMAPHORE: u32 = 1;

/// serde_json 任意精度数字使用的透明 Serde 结构名称
const SERDE_JSON_NUMBER_TOKEN: &str = "$serde_json::private::Number";

/// serde_json 原始 JSON 值使用的透明 Serde 结构名称
const SERDE_JSON_RAW_VALUE_TOKEN: &str = "$serde_json::private::RawValue";

/// 参数类型包含序列化时可能阻塞的同步锁
const BLOCKING_LOCK_PARAMETER_ERROR: &str = "参数包含序列化时可能阻塞的同步锁类型";

/// 缓存参数类型包含可变引用或可变裸指针
const MUTABLE_CACHE_PARAMETER_ERROR: &str = "缓存参数包含可变引用或可变裸指针";

/// 参数类型包含无法生成稳定键的 trait object
const TRAIT_OBJECT_PARAMETER_ERROR: &str = "参数包含无法生成稳定键的 trait object";

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
            is_use_parameter: DEFAULT_CONCURRENCY_LIMIT_USE_PARAMETER,
            is_block: DEFAULT_CONCURRENCY_LIMIT_IS_BLOCK,
            expiry_seconds: DEFAULT_CONCURRENCY_LIMIT_EXPIRY_SECONDS,
            semaphore: DEFAULT_CONCURRENCY_LIMIT_SEMAPHORE,
        }
    }
}

/// 为参与稳定方法键的类型提供显式且跨构建可维护的标签
pub trait StableKeyType {
    /// 返回类型对应的稳定标签
    fn stable_key_type() -> String;
}

/// 获取类型显式声明的稳定标签
pub fn stable_key_type<T>() -> String
where
    T: StableKeyType + ?Sized,
{
    T::stable_key_type()
}

/// 根据值引用推断并获取匿名泛型参数的稳定类型标签
#[doc(hidden)]
pub fn stable_key_type_of<T>(_: &T) -> String
where
    T: StableKeyType + ?Sized,
{
    T::stable_key_type()
}

/// 将无序集合显式包装为按规范 JSON 排序的参数键值
pub struct UnorderedKey<I> {
    values: I,
}

/// 创建用于参数键生成的无序集合包装
pub fn unordered_key<I>(values: I) -> UnorderedKey<I> {
    UnorderedKey { values }
}

impl<I> Serialize for UnorderedKey<I>
where
    I: Clone + IntoIterator,
    I::Item: Serialize,
{
    /// 按元素的规范 JSON 排序后序列化无序集合
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        const MAX_ITEMS: usize = 10_000;
        const MAX_BYTES: usize = 1024 * 1024;

        let mut values = Vec::new();
        let mut total_bytes = 0usize;

        for value in self.values.clone() {
            if values.len() >= MAX_ITEMS {
                return Err(S::Error::custom(format!(
                    "无序参数集合超过 {MAX_ITEMS} 项限制"
                )));
            }

            let key = arguments_key(&value).map_err(S::Error::custom)?;
            total_bytes = total_bytes
                .checked_add(key.len())
                .filter(|length| *length <= MAX_BYTES)
                .ok_or_else(|| {
                    S::Error::custom(format!("无序参数集合超过 {MAX_BYTES} 字节限制"))
                })?;
            let value = serde_json::from_str::<serde_json::Value>(&key)
                .map_err(|error| S::Error::custom(format!("无序参数元素无法解析: {error}")))?;
            values.push((key, value));
        }

        values.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let mut sequence = serializer.serialize_seq(Some(values.len()))?;
        for (_, value) in values {
            sequence.serialize_element(&value)?;
        }
        sequence.end()
    }
}

macro_rules! impl_stable_key_type {
    ($($type:ty => $tag:literal),+ $(,)?) => {
        $(
            impl StableKeyType for $type {
                fn stable_key_type() -> String {
                    $tag.to_string()
                }
            }
        )+
    };
}

impl_stable_key_type!(
    () => "rust:unit",
    bool => "rust:bool",
    char => "rust:char",
    i8 => "rust:i8",
    i16 => "rust:i16",
    i32 => "rust:i32",
    i64 => "rust:i64",
    i128 => "rust:i128",
    isize => "rust:isize",
    u8 => "rust:u8",
    u16 => "rust:u16",
    u32 => "rust:u32",
    u64 => "rust:u64",
    u128 => "rust:u128",
    usize => "rust:usize",
    f32 => "rust:f32",
    f64 => "rust:f64",
    str => "rust:str",
    String => "rust:string",
    serde_json::Value => "serde-json:value",
    uuid::Uuid => "uuid:uuid",
);

impl<T> StableKeyType for &T
where
    T: StableKeyType + ?Sized,
{
    /// 返回不可变引用的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type("rust:ref", &[T::stable_key_type()])
    }
}

impl<T> StableKeyType for &mut T
where
    T: StableKeyType + ?Sized,
{
    /// 返回可变引用的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type("rust:ref-mut", &[T::stable_key_type()])
    }
}

macro_rules! impl_stable_key_wrapper {
    ($type:ident, $tag:literal) => {
        impl<T> StableKeyType for $type<T>
        where
            T: StableKeyType,
        {
            fn stable_key_type() -> String {
                compose_stable_key_type($tag, &[T::stable_key_type()])
            }
        }
    };
}

impl_stable_key_wrapper!(Option, "rust:option");
impl_stable_key_wrapper!(Box, "rust:box");
impl_stable_key_wrapper!(Rc, "rust:rc");
impl_stable_key_wrapper!(Arc, "rust:arc");
impl_stable_key_wrapper!(Vec, "rust:vec");
impl_stable_key_wrapper!(VecDeque, "rust:vec-deque");
impl_stable_key_wrapper!(LinkedList, "rust:linked-list");
impl_stable_key_wrapper!(BTreeSet, "rust:btree-set");

impl<T, S> StableKeyType for HashSet<T, S>
where
    T: StableKeyType,
    S: BuildHasher,
{
    /// 返回哈希集合的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type("rust:hash-set", &[T::stable_key_type()])
    }
}

impl<K, V> StableKeyType for BTreeMap<K, V>
where
    K: StableKeyType,
    V: StableKeyType,
{
    /// 返回有序字典的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type(
            "rust:btree-map",
            &[K::stable_key_type(), V::stable_key_type()],
        )
    }
}

impl<K, V, S> StableKeyType for HashMap<K, V, S>
where
    K: StableKeyType,
    V: StableKeyType,
    S: BuildHasher,
{
    /// 返回哈希字典的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type(
            "rust:hash-map",
            &[K::stable_key_type(), V::stable_key_type()],
        )
    }
}

impl<T, E> StableKeyType for Result<T, E>
where
    T: StableKeyType,
    E: StableKeyType,
{
    /// 返回结果类型的稳定标签
    fn stable_key_type() -> String {
        compose_stable_key_type("rust:result", &[T::stable_key_type(), E::stable_key_type()])
    }
}

impl<T, const N: usize> StableKeyType for [T; N]
where
    T: StableKeyType,
{
    /// 返回固定长度数组的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type("rust:array", &[T::stable_key_type(), format!("length:{N}")])
    }
}

impl<T> StableKeyType for [T]
where
    T: StableKeyType,
{
    /// 返回切片的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type("rust:slice", &[T::stable_key_type()])
    }
}

macro_rules! impl_stable_key_tuple {
    ($tag:literal; $($type:ident),+) => {
        impl<$($type),+> StableKeyType for ($($type,)+)
        where
            $($type: StableKeyType),+
        {
            fn stable_key_type() -> String {
                compose_stable_key_type($tag, &[$($type::stable_key_type()),+])
            }
        }
    };
}

impl_stable_key_tuple!("rust:tuple1"; T1);
impl_stable_key_tuple!("rust:tuple2"; T1, T2);
impl_stable_key_tuple!("rust:tuple3"; T1, T2, T3);
impl_stable_key_tuple!("rust:tuple4"; T1, T2, T3, T4);
impl_stable_key_tuple!("rust:tuple5"; T1, T2, T3, T4, T5);
impl_stable_key_tuple!("rust:tuple6"; T1, T2, T3, T4, T5, T6);
impl_stable_key_tuple!("rust:tuple7"; T1, T2, T3, T4, T5, T6, T7);
impl_stable_key_tuple!("rust:tuple8"; T1, T2, T3, T4, T5, T6, T7, T8);

/// 使用长度前缀组合用户类型与泛型参数的稳定标签
pub fn compose_stable_key_type(prefix: &str, parts: &[String]) -> String {
    let mut tag = format!("{}:{prefix}", prefix.len());
    for part in parts {
        tag.push('|');
        tag.push_str(&part.len().to_string());
        tag.push(':');
        tag.push_str(part);
    }
    tag
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

/// 在异常日志启用时格式化并记录方法调用错误
pub fn log_exception_debug(method: &str, trace_id: &str, duration_ms: u128, error: &impl Debug) {
    if !tracing::event_enabled!(
        tracing::Level::ERROR,
        trace_id,
        method,
        duration_ms,
        error,
        event
    ) {
        return;
    }

    let error = error_summary(error);
    log_exception(method, trace_id, duration_ms, &error);
}

/// 重试间隔等待
///
/// delay_ms 为 0 时直接返回，不产生任何调度开销
pub async fn retry_sleep(delay_ms: u64) {
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

/// 校验重试方法的声明参数类型实现 Copy
#[doc(hidden)]
pub fn assert_retry_parameter_type_copy<T>()
where
    T: Copy,
{
}

/// 校验重试方法的匿名 impl Trait 参数实现 Copy
#[doc(hidden)]
pub fn assert_retry_parameter_copy<T>(_: &T)
where
    T: Copy,
{
}

/// 将秒级重试等待时间转换为毫秒
pub fn retry_delay_ms_from_seconds(delay_seconds: u64) -> u64 {
    delay_seconds.saturating_mul(1000)
}

/// 生成适合写入日志的有界错误摘要
pub fn error_summary(error: &impl std::fmt::Debug) -> String {
    format_error_summary(format_args!("{error:?}"))
}

/// 生成适合写入重试日志的有界错误摘要
pub fn retry_error_summary(error: &impl std::fmt::Debug) -> String {
    error_summary(error)
}

/// 生成适合写入日志的有界展示文本
fn display_summary(value: &dyn Display) -> String {
    format_error_summary(format_args!("{value}"))
}

/// 将格式化参数写入有界且无控制字符的摘要
fn format_error_summary(arguments: fmt::Arguments<'_>) -> String {
    const MAX_CHARS: usize = 512;

    let mut writer = ErrorSummaryWriter::new(MAX_CHARS);
    let _ = fmt::write(&mut writer, arguments);
    writer.finish()
}

/// 在格式化阶段限制错误摘要的字符数量
struct ErrorSummaryWriter {
    summary: String,
    max_chars: usize,
    written_chars: usize,
    truncated: bool,
}

impl ErrorSummaryWriter {
    /// 创建有界错误摘要写入器
    fn new(max_chars: usize) -> Self {
        Self {
            summary: String::with_capacity(max_chars),
            max_chars,
            written_chars: 0,
            truncated: false,
        }
    }

    /// 完成摘要并在截断时追加标记
    fn finish(mut self) -> String {
        if self.truncated {
            self.summary.push('…');
        }

        self.summary
    }
}

impl fmt::Write for ErrorSummaryWriter {
    /// 写入不超过字符上限的错误文本并清理控制字符
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for character in value.chars() {
            if self.written_chars >= self.max_chars {
                self.truncated = true;
                return Err(fmt::Error);
            }

            self.written_chars += 1;
            if character.is_control() {
                if !self.summary.ends_with(' ') {
                    self.summary.push(' ');
                }
            } else {
                self.summary.push(character);
            }
        }

        Ok(())
    }
}

/// 记录即将执行下一次重试
pub fn log_retry_scheduled(
    method: &str,
    trace_id: &str,
    retry: u64,
    delay_ms: u64,
    error_type: &str,
    error: &str,
) {
    tracing::warn!(
        method,
        trace_id,
        retry,
        delay_ms,
        error_type,
        error,
        event = "retry_scheduled"
    );
}

/// 在警告日志启用时格式化并记录即将执行的下一次重试
pub fn log_retry_scheduled_debug(
    method: &str,
    trace_id: &str,
    retry: u64,
    delay_ms: u64,
    error: &impl Debug,
) {
    if !tracing::event_enabled!(
        tracing::Level::WARN,
        method,
        trace_id,
        retry,
        delay_ms,
        error_type,
        error,
        event
    ) {
        return;
    }

    let error_type = std::any::type_name_of_val(error);
    let error = retry_error_summary(error);
    log_retry_scheduled(method, trace_id, retry, delay_ms, error_type, &error);
}

/// 记录重试次数已经耗尽
pub fn log_retry_exhausted(
    method: &str,
    trace_id: &str,
    retries: u64,
    error_type: &str,
    error: &str,
) {
    tracing::error!(
        method,
        trace_id,
        retries,
        error_type,
        error,
        event = "retry_exhausted"
    );
}

/// 在错误日志启用时格式化并记录重试次数耗尽错误
pub fn log_retry_exhausted_debug(method: &str, trace_id: &str, retries: u64, error: &impl Debug) {
    if !tracing::event_enabled!(
        tracing::Level::ERROR,
        method,
        trace_id,
        retries,
        error_type,
        error,
        event
    ) {
        return;
    }

    let error_type = std::any::type_name_of_val(error);
    let error = retry_error_summary(error);
    log_retry_exhausted(method, trace_id, retries, error_type, &error);
}

/// 记录缓存读取、写入或防击穿锁故障并继续执行业务
pub fn log_cache_degraded(method: &str, operation: &str, error: &dyn Display) {
    if !tracing::enabled!(tracing::Level::WARN) {
        return;
    }

    let error = display_summary(error);
    tracing::warn!(method, operation, error, event = "cache_degraded");
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

impl<T> AsDistributedLockRef for T
where
    T: DistributedLock,
{
    /// 返回具体分布式锁或透明包装的 trait object 引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock {
        self
    }
}

impl AsDistributedLockRef for dyn DistributedLock {
    /// 返回动态分布式锁自身引用
    fn as_distributed_lock_ref(&self) -> &dyn DistributedLock {
        self
    }
}

/// 获取混合缓存引用
pub fn hybrid_cache_ref<T>(cache: &T) -> &HybridCache
where
    T: AsHybridCacheRef + ?Sized,
{
    cache.as_hybrid_cache_ref()
}

/// 使用返回类型自身的相等语义校验缓存值往返结果并写入缓存
pub async fn set_cacheable_json<T>(
    cache: &HybridCache,
    key: &str,
    value: &T,
    l2_ttl_seconds: u64,
) -> Result<(), HybridCacheError>
where
    T: Serialize + DeserializeOwned + PartialEq,
{
    cache
        .set_json_with_round_trip_validator(
            key,
            value,
            l2_ttl_seconds,
            |original_value, round_trip_value| Ok(original_value == round_trip_value),
        )
        .await
}

/// 获取分布式锁引用
pub fn distributed_lock_ref<T>(lock: &T) -> &dyn DistributedLock
where
    T: AsDistributedLockRef + ?Sized,
{
    lock.as_distributed_lock_ref()
}

/// 根据显式作用域与稳定类型标签生成方法标识
pub fn stable_method_key(
    scope: &str,
    self_type: Option<&str>,
    generic_types: &[String],
    const_generics: &[String],
) -> String {
    let mut key = format!("source-generator|scope={}:{}", scope.len(), scope);

    if let Some(self_type) = self_type {
        append_scoped_key_dimension(&mut key, "self", [self_type]);
    }
    append_scoped_key_dimension(
        &mut key,
        "generic",
        generic_types.iter().map(String::as_str),
    );
    append_scoped_key_dimension(&mut key, "const", const_generics.iter().map(String::as_str));
    key
}

/// 使用数量和长度前缀追加一组作用域键维度
fn append_scoped_key_dimension<'a>(
    key: &mut String,
    label: &str,
    values: impl IntoIterator<Item = &'a str>,
) {
    let values = values.into_iter().collect::<Vec<_>>();
    key.push('|');
    key.push_str(label);
    key.push('=');
    key.push_str(&values.len().to_string());

    for value in values {
        key.push(';');
        key.push_str(&value.len().to_string());
        key.push(':');
        key.push_str(value);
    }
}

/// 将调用参数序列化为有边界的规范 JSON
///
/// 无序集合必须通过 unordered_key 显式包装后参与参数键生成
/// 非有限浮点数会被拒绝，正负零会被规范化为相同数值
/// 自定义 Serialize 实现需要自行保证内部无序集合的输出顺序稳定
pub fn arguments_key<T>(args: &T) -> Result<String, InvocationKeyError>
where
    T: Serialize + ?Sized,
{
    serialize_arguments_key(args, ParameterKeySafetyPolicy::default())
}

/// 宏生成的参数值包装
#[doc(hidden)]
pub struct InvocationArguments<T>(pub T);

/// 校验缓存方法原始参数类型不会因缓存命中丢失可变副作用
#[doc(hidden)]
pub fn validate_cacheable_parameter_type<T>(_: &T) -> Result<(), InvocationKeyError>
where
    T: ?Sized,
{
    ParameterKeySafetyPolicy::cacheable_parameter()
        .validate::<T>()
        .map_err(InvocationKeyError::new)
}

/// 将宏生成的参数值序列化为有边界的规范 JSON
#[doc(hidden)]
pub fn invocation_arguments_key<T>(
    args: &InvocationArguments<T>,
) -> Result<String, InvocationKeyError>
where
    T: Serialize,
{
    serialize_arguments_key(&args.0, ParameterKeySafetyPolicy::invocation_key())
}

/// 将宏生成的缓存参数元组序列化为有边界的规范 JSON
#[doc(hidden)]
pub fn cacheable_invocation_arguments_key<T>(
    args: &InvocationArguments<T>,
) -> Result<String, InvocationKeyError>
where
    T: Serialize,
{
    serialize_arguments_key(&args.0, ParameterKeySafetyPolicy::cacheable_key())
}

/// 按指定参数语义生成规范 JSON
fn serialize_arguments_key<T>(
    args: &T,
    safety_policy: ParameterKeySafetyPolicy,
) -> Result<String, InvocationKeyError>
where
    T: Serialize + ?Sized,
{
    const MAX_DEPTH: usize = 64;
    const MAX_TRAVERSAL_DEPTH: usize = 128;
    const MAX_ITEMS: usize = 10_000;
    const MAX_BYTES: usize = 1024 * 1024;

    safety_policy
        .validate::<T>()
        .map_err(InvocationKeyError::new)?;

    let mut writer = LimitedJsonWriter::new(MAX_BYTES);
    let mut serializer = serde_json::Serializer::new(&mut writer);
    let budget = SerializationBudget::new(MAX_DEPTH, MAX_TRAVERSAL_DEPTH, MAX_ITEMS, safety_policy);
    if let Err(error) = args.serialize(CanonicalJsonSerializer::new(&mut serializer, &budget, 0, 0))
    {
        if writer.exceeded() {
            return Err(InvocationKeyError::new(format!(
                "参数 JSON 超过 {MAX_BYTES} 字节限制"
            )));
        }

        return Err(InvocationKeyError::new(format!(
            "参数无法序列化为 JSON: {}",
            display_summary(&error)
        )));
    }

    validate_json_complexity(writer.as_bytes(), MAX_DEPTH, MAX_ITEMS)?;

    let value = serde_json::from_slice(writer.as_bytes())
        .map_err(|error| InvocationKeyError::new(format!("参数 JSON 无法解析: {error}")))?;
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

/// 参数键序列化前需要执行的类型安全策略
#[derive(Clone, Copy)]
struct ParameterKeySafetyPolicy {
    reject_blocking_locks: bool,
    reject_mutable_types: bool,
    reject_trait_objects: bool,
}

impl ParameterKeySafetyPolicy {
    /// 创建宏调用参数键使用的类型安全策略
    fn invocation_key() -> Self {
        Self {
            reject_blocking_locks: true,
            reject_mutable_types: false,
            reject_trait_objects: true,
        }
    }

    /// 创建缓存键参数使用的类型安全策略
    fn cacheable_key() -> Self {
        Self {
            reject_blocking_locks: true,
            reject_mutable_types: true,
            reject_trait_objects: true,
        }
    }

    /// 创建缓存方法原始参数使用的类型安全策略
    fn cacheable_parameter() -> Self {
        Self {
            reject_blocking_locks: false,
            reject_mutable_types: true,
            reject_trait_objects: false,
        }
    }

    /// 检查指定参数类型是否可以安全进入同步序列化流程
    fn validate<T>(self) -> Result<(), &'static str>
    where
        T: ?Sized,
    {
        if self.reject_blocking_locks && contains_runtime_blocking_lock(std::any::type_name::<T>())
        {
            return Err(BLOCKING_LOCK_PARAMETER_ERROR);
        }
        if self.reject_trait_objects && contains_runtime_trait_object(std::any::type_name::<T>()) {
            return Err(TRAIT_OBJECT_PARAMETER_ERROR);
        }
        if self.reject_mutable_types && contains_runtime_mutable_type(std::any::type_name::<T>()) {
            return Err(MUTABLE_CACHE_PARAMETER_ERROR);
        }

        Ok(())
    }
}

impl Default for ParameterKeySafetyPolicy {
    /// 默认拒绝会在 Serde 阶段同步等待的锁类型
    fn default() -> Self {
        Self {
            reject_blocking_locks: true,
            reject_mutable_types: false,
            reject_trait_objects: false,
        }
    }
}

/// 按 Rust 类型标记边界查找可变引用与可变裸指针
fn contains_runtime_mutable_type(type_name: &str) -> bool {
    let bytes = type_name.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        if matches!(bytes[index], b'&' | b'*') {
            let mut marker_index = index + 1;
            while bytes.get(marker_index).is_some_and(u8::is_ascii_whitespace) {
                marker_index += 1;
            }

            if bytes.get(marker_index..marker_index + 3) == Some(b"mut")
                && bytes
                    .get(marker_index + 3)
                    .is_some_and(u8::is_ascii_whitespace)
            {
                return true;
            }
        }

        index += 1;
    }

    false
}

/// 按 Rust 标识符边界查找运行时类型名称中的 dyn 关键字
fn contains_runtime_trait_object(type_name: &str) -> bool {
    let bytes = type_name.as_bytes();
    let mut index = 0usize;

    while index + 3 <= bytes.len() {
        if bytes.get(index..index + 3) == Some(b"dyn") {
            let before_is_identifier = index
                .checked_sub(1)
                .and_then(|position| bytes.get(position))
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
            let followed_by_trait_name = bytes.get(index + 3).is_some_and(u8::is_ascii_whitespace);
            if !before_is_identifier && followed_by_trait_name {
                return true;
            }
        }

        index += 1;
    }

    false
}

/// 判断运行时类型中是否包含序列化时会同步阻塞的锁
fn contains_runtime_blocking_lock(type_name: &str) -> bool {
    const BLOCKING_LOCK_PATHS: [&str; 7] = [
        "std::sync::Mutex<",
        "std::sync::RwLock<",
        "std::sync::poison::mutex::Mutex<",
        "std::sync::poison::rwlock::RwLock<",
        "lock_api::mutex::Mutex<",
        "lock_api::rwlock::RwLock<",
        "lock_api::remutex::ReentrantMutex<",
    ];

    BLOCKING_LOCK_PATHS.iter().any(|lock_path| {
        type_name.match_indices(lock_path).any(|(index, _)| {
            type_name[..index]
                .chars()
                .next_back()
                .is_none_or(|character| {
                    !character.is_alphanumeric() && !matches!(character, '_' | ':')
                })
        })
    })
}

/// 限制首次参数序列化允许访问的 JSON 深度和项目数量
struct SerializationBudget {
    item_count: Cell<usize>,
    max_depth: usize,
    max_traversal_depth: usize,
    max_items: usize,
    safety_policy: ParameterKeySafetyPolicy,
}

impl SerializationBudget {
    /// 创建已经包含根节点的序列化预算
    fn new(
        max_depth: usize,
        max_traversal_depth: usize,
        max_items: usize,
        safety_policy: ParameterKeySafetyPolicy,
    ) -> Self {
        Self {
            item_count: Cell::new(1),
            max_depth,
            max_traversal_depth,
            max_items,
            safety_policy,
        }
    }

    /// 在序列化子节点前检查并消耗一次预算
    fn enter<E>(&self, depth: usize) -> Result<(), E>
    where
        E: SerializeError,
    {
        self.enter_many(depth, 1)
    }

    /// 在序列化一批同层子节点前检查并消耗预算
    fn enter_many<E>(&self, depth: usize, count: usize) -> Result<(), E>
    where
        E: SerializeError,
    {
        if count == 0 {
            return Ok(());
        }

        if depth > self.max_depth {
            return Err(E::custom(format!(
                "参数 JSON 超过 {} 层深度限制",
                self.max_depth
            )));
        }

        let item_count = self
            .item_count
            .get()
            .checked_add(count)
            .ok_or_else(|| E::custom("参数 JSON 项目计数溢出"))?;
        if item_count > self.max_items {
            return Err(E::custom(format!(
                "参数 JSON 超过 {} 项限制",
                self.max_items
            )));
        }

        self.item_count.set(item_count);
        Ok(())
    }

    /// 检查 Serde 嵌套调用深度
    fn check_traversal<E>(&self, traversal_depth: usize) -> Result<(), E>
    where
        E: SerializeError,
    {
        if traversal_depth > self.max_traversal_depth {
            return Err(E::custom(format!(
                "参数 Serde 遍历超过 {} 层调用深度限制",
                self.max_traversal_depth
            )));
        }

        Ok(())
    }
}

/// 在递归序列化参数时校验复杂度并规范化浮点数
struct CanonicalJsonSerializer<'a, S> {
    serializer: S,
    budget: &'a SerializationBudget,
    depth: usize,
    traversal_depth: usize,
}

impl<'a, S> CanonicalJsonSerializer<'a, S> {
    /// 创建规范 JSON 序列化包装器
    fn new(
        serializer: S,
        budget: &'a SerializationBudget,
        depth: usize,
        traversal_depth: usize,
    ) -> Self {
        Self {
            serializer,
            budget,
            depth,
            traversal_depth,
        }
    }
}

/// 将嵌套值继续交给规范 JSON 序列化器
struct CanonicalJsonValue<'a, 'b, T: ?Sized> {
    value: &'a T,
    budget: &'b SerializationBudget,
    depth: usize,
    traversal_depth: usize,
}

impl<'a, 'b, T: ?Sized> CanonicalJsonValue<'a, 'b, T> {
    /// 创建嵌套值包装器
    fn new(
        value: &'a T,
        budget: &'b SerializationBudget,
        depth: usize,
        traversal_depth: usize,
    ) -> Self {
        Self {
            value,
            budget,
            depth,
            traversal_depth,
        }
    }
}

impl<T> Serialize for CanonicalJsonValue<'_, '_, T>
where
    T: Serialize + ?Sized,
{
    /// 使用规范 JSON 序列化器递归处理当前值
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.budget
            .check_traversal::<S::Error>(self.traversal_depth)?;
        self.budget
            .safety_policy
            .validate::<T>()
            .map_err(S::Error::custom)?;
        self.value.serialize(CanonicalJsonSerializer::new(
            serializer,
            self.budget,
            self.depth,
            self.traversal_depth,
        ))
    }
}

impl<'a, S> Serializer for CanonicalJsonSerializer<'a, S>
where
    S: Serializer,
{
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = CanonicalSerializeSeq<'a, S::SerializeSeq>;
    type SerializeTuple = CanonicalSerializeTuple<'a, S::SerializeTuple>;
    type SerializeTupleStruct = CanonicalSerializeTupleStruct<'a, S::SerializeTupleStruct>;
    type SerializeTupleVariant = CanonicalSerializeTupleVariant<'a, S::SerializeTupleVariant>;
    type SerializeMap = CanonicalSerializeMap<'a, S::SerializeMap>;
    type SerializeStruct = CanonicalSerializeStruct<'a, S::SerializeStruct>;
    type SerializeStructVariant = CanonicalSerializeStructVariant<'a, S::SerializeStructVariant>;

    /// 序列化布尔值
    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_bool(value)
    }

    /// 序列化 i8
    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i8(value)
    }

    /// 序列化 i16
    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i16(value)
    }

    /// 序列化 i32
    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i32(value)
    }

    /// 序列化 i64
    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i64(value)
    }

    /// 序列化 i128
    fn serialize_i128(self, value: i128) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_i128(value)
    }

    /// 序列化 u8
    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u8(value)
    }

    /// 序列化 u16
    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u16(value)
    }

    /// 序列化 u32
    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u32(value)
    }

    /// 序列化 u64
    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u64(value)
    }

    /// 序列化 u128
    fn serialize_u128(self, value: u128) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_u128(value)
    }

    /// 序列化有限 f32 并统一正负零
    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        if !value.is_finite() {
            return Err(<S::Error as SerializeError>::custom(
                "参数包含非有限 f32 浮点数",
            ));
        }

        self.serializer
            .serialize_f32(if value == 0.0 { 0.0 } else { value })
    }

    /// 序列化有限 f64 并统一正负零
    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        if !value.is_finite() {
            return Err(<S::Error as SerializeError>::custom(
                "参数包含非有限 f64 浮点数",
            ));
        }

        self.serializer
            .serialize_f64(if value == 0.0 { 0.0 } else { value })
    }

    /// 序列化字符
    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_char(value)
    }

    /// 序列化字符串
    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_str(value)
    }

    /// 流式格式化并序列化展示值
    fn collect_str<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Display + ?Sized,
    {
        self.serializer.collect_str(value)
    }

    /// 序列化字节
    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        self.budget
            .enter_many::<S::Error>(child_depth, value.len())?;
        self.serializer.serialize_bytes(value)
    }

    /// 序列化空可选值
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_none()
    }

    /// 序列化非空可选值
    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_some(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.depth,
            child_traversal_depth,
        ))
    }

    /// 序列化单元值
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_unit()
    }

    /// 序列化单元结构
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serializer.serialize_unit_struct(name)
    }

    /// 序列化单元枚举变体
    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.serializer
            .serialize_unit_variant(name, variant_index, variant)
    }

    /// 序列化新类型结构
    fn serialize_newtype_struct<T>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_newtype_struct(
            name,
            &CanonicalJsonValue::new(value, self.budget, self.depth, child_traversal_depth),
        )
    }

    /// 序列化新类型枚举变体
    fn serialize_newtype_variant<T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.budget.enter::<S::Error>(child_depth)?;
        self.serializer.serialize_newtype_variant(
            name,
            variant_index,
            variant,
            &CanonicalJsonValue::new(value, self.budget, child_depth, child_traversal_depth),
        )
    }

    /// 开始序列化数组
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_seq(len).map(|sequence| {
            CanonicalSerializeSeq::new(sequence, self.budget, child_depth, child_traversal_depth)
        })
    }

    /// 开始序列化元组
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_tuple(len).map(|tuple| {
            CanonicalSerializeTuple::new(tuple, self.budget, child_depth, child_traversal_depth)
        })
    }

    /// 开始序列化元组结构
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer
            .serialize_tuple_struct(name, len)
            .map(|tuple| {
                CanonicalSerializeTupleStruct::new(
                    tuple,
                    self.budget,
                    child_depth,
                    child_traversal_depth,
                )
            })
    }

    /// 开始序列化元组枚举变体
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        let container_depth = self.depth.saturating_add(1);
        let child_depth = self.depth.saturating_add(2);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.budget.enter::<S::Error>(container_depth)?;
        self.serializer
            .serialize_tuple_variant(name, variant_index, variant, len)
            .map(|tuple| {
                CanonicalSerializeTupleVariant::new(
                    tuple,
                    self.budget,
                    child_depth,
                    child_traversal_depth,
                )
            })
    }

    /// 开始序列化字典
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer.serialize_map(len).map(|map| {
            CanonicalSerializeMap::new(map, self.budget, child_depth, child_traversal_depth)
        })
    }

    /// 开始序列化结构
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        let transparent_token = serde_json_transparent_struct_token(name, len);
        let child_depth = self.depth.saturating_add(1);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.serializer
            .serialize_struct(name, len)
            .map(|structure| {
                CanonicalSerializeStruct::new(
                    structure,
                    self.budget,
                    self.depth,
                    child_depth,
                    child_traversal_depth,
                    transparent_token,
                )
            })
    }

    /// 开始序列化结构枚举变体
    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let container_depth = self.depth.saturating_add(1);
        let child_depth = self.depth.saturating_add(2);
        let child_traversal_depth = self.traversal_depth.saturating_add(1);
        self.budget.enter::<S::Error>(container_depth)?;
        self.serializer
            .serialize_struct_variant(name, variant_index, variant, len)
            .map(|structure| {
                CanonicalSerializeStructVariant::new(
                    structure,
                    self.budget,
                    child_depth,
                    child_traversal_depth,
                )
            })
    }

    /// 返回底层序列化器的人类可读标识
    fn is_human_readable(&self) -> bool {
        self.serializer.is_human_readable()
    }
}

/// 递归规范化数组元素
struct CanonicalSerializeSeq<'a, S> {
    sequence: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeSeq<'a, S> {
    /// 创建数组序列化包装器
    fn new(
        sequence: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            sequence,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeSeq for CanonicalSerializeSeq<'_, S>
where
    S: SerializeSeq,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化数组元素
    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.sequence.serialize_element(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束数组序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.sequence.end()
    }
}

/// 递归规范化元组元素
struct CanonicalSerializeTuple<'a, S> {
    tuple: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeTuple<'a, S> {
    /// 创建元组序列化包装器
    fn new(
        tuple: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            tuple,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeTuple for CanonicalSerializeTuple<'_, S>
where
    S: SerializeTuple,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化元组元素
    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.tuple.serialize_element(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束元组序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.tuple.end()
    }
}

/// 递归规范化元组结构字段
struct CanonicalSerializeTupleStruct<'a, S> {
    tuple: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeTupleStruct<'a, S> {
    /// 创建元组结构序列化包装器
    fn new(
        tuple: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            tuple,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeTupleStruct for CanonicalSerializeTupleStruct<'_, S>
where
    S: SerializeTupleStruct,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化元组结构字段
    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.tuple.serialize_field(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束元组结构序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.tuple.end()
    }
}

/// 递归规范化元组枚举变体字段
struct CanonicalSerializeTupleVariant<'a, S> {
    tuple: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeTupleVariant<'a, S> {
    /// 创建元组枚举变体序列化包装器
    fn new(
        tuple: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            tuple,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeTupleVariant for CanonicalSerializeTupleVariant<'_, S>
where
    S: SerializeTupleVariant,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化元组枚举变体字段
    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.tuple.serialize_field(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束元组枚举变体序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.tuple.end()
    }
}

/// 递归规范化字典键和值
struct CanonicalSerializeMap<'a, S> {
    map: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeMap<'a, S> {
    /// 创建字典序列化包装器
    fn new(
        map: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            map,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeMap for CanonicalSerializeMap<'_, S>
where
    S: SerializeMap,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化字典键
    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.map.serialize_key(&CanonicalJsonValue::new(
            key,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 序列化字典值
    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.map.serialize_value(&CanonicalJsonValue::new(
            value,
            self.budget,
            self.child_depth,
            self.child_traversal_depth,
        ))
    }

    /// 结束字典序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.map.end()
    }
}

/// 识别 serde_json 最终输出为单个 JSON 值的私有透明结构协议
fn serde_json_transparent_struct_token(name: &'static str, len: usize) -> Option<&'static str> {
    if len == 1 && matches!(name, SERDE_JSON_NUMBER_TOKEN | SERDE_JSON_RAW_VALUE_TOKEN) {
        Some(name)
    } else {
        None
    }
}

/// 递归规范化结构字段
struct CanonicalSerializeStruct<'a, S> {
    structure: S,
    budget: &'a SerializationBudget,
    parent_depth: usize,
    child_depth: usize,
    child_traversal_depth: usize,
    transparent_token: Option<&'static str>,
}

impl<'a, S> CanonicalSerializeStruct<'a, S> {
    /// 创建结构序列化包装器
    fn new(
        structure: S,
        budget: &'a SerializationBudget,
        parent_depth: usize,
        child_depth: usize,
        child_traversal_depth: usize,
        transparent_token: Option<&'static str>,
    ) -> Self {
        Self {
            structure,
            budget,
            parent_depth,
            child_depth,
            child_traversal_depth,
            transparent_token,
        }
    }
}

impl<S> SerializeStruct for CanonicalSerializeStruct<'_, S>
where
    S: SerializeStruct,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化结构字段
    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        let is_transparent_value = self.transparent_token == Some(key);
        let value_depth = if is_transparent_value {
            self.parent_depth
        } else {
            self.budget.enter::<Self::Error>(self.child_depth)?;
            self.child_depth
        };
        self.structure.serialize_field(
            key,
            &CanonicalJsonValue::new(value, self.budget, value_depth, self.child_traversal_depth),
        )
    }

    /// 结束结构序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.structure.end()
    }
}

/// 递归规范化结构枚举变体字段
struct CanonicalSerializeStructVariant<'a, S> {
    structure: S,
    budget: &'a SerializationBudget,
    child_depth: usize,
    child_traversal_depth: usize,
}

impl<'a, S> CanonicalSerializeStructVariant<'a, S> {
    /// 创建结构枚举变体序列化包装器
    fn new(
        structure: S,
        budget: &'a SerializationBudget,
        child_depth: usize,
        child_traversal_depth: usize,
    ) -> Self {
        Self {
            structure,
            budget,
            child_depth,
            child_traversal_depth,
        }
    }
}

impl<S> SerializeStructVariant for CanonicalSerializeStructVariant<'_, S>
where
    S: SerializeStructVariant,
{
    type Ok = S::Ok;
    type Error = S::Error;

    /// 序列化结构枚举变体字段
    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        self.budget.enter::<Self::Error>(self.child_depth)?;
        self.structure.serialize_field(
            key,
            &CanonicalJsonValue::new(
                value,
                self.budget,
                self.child_depth,
                self.child_traversal_depth,
            ),
        )
    }

    /// 结束结构枚举变体序列化
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.structure.end()
    }
}

/// 限制参数 JSON 序列化期间允许写入的最大字节数
struct LimitedJsonWriter {
    buffer: Vec<u8>,
    max_bytes: usize,
    exceeded: bool,
}

impl LimitedJsonWriter {
    /// 创建有界 JSON 写入器
    fn new(max_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_bytes,
            exceeded: false,
        }
    }

    /// 返回是否因为字节限制拒绝了写入
    fn exceeded(&self) -> bool {
        self.exceeded
    }

    /// 获取已经写入的 JSON 字节
    fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }
}

impl Write for LimitedJsonWriter {
    /// 写入不超过配置上限的 JSON 字节
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next_len) = self.buffer.len().checked_add(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("参数 JSON 字节数溢出"));
        };

        if next_len > self.max_bytes {
            self.exceeded = true;
            return Err(io::Error::other("参数 JSON 超过字节限制"));
        }

        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// 刷新内存写入器
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// 在构建 JSON 值树之前按原始 JSON 语法检查深度和节点数量
fn validate_json_complexity(
    bytes: &[u8],
    max_depth: usize,
    max_items: usize,
) -> Result<(), InvocationKeyError> {
    JsonComplexityScanner {
        bytes,
        index: 0,
        item_count: 0,
        max_depth,
        max_items,
    }
    .validate()
}

/// 原始 JSON 复杂度扫描器
struct JsonComplexityScanner<'a> {
    bytes: &'a [u8],
    index: usize,
    item_count: usize,
    max_depth: usize,
    max_items: usize,
}

impl JsonComplexityScanner<'_> {
    /// 扫描唯一根值并确认没有额外内容
    fn validate(mut self) -> Result<(), InvocationKeyError> {
        self.skip_whitespace();
        self.parse_value(0)?;
        self.skip_whitespace();
        if self.index != self.bytes.len() {
            return Err(self.error("参数 JSON 包含额外内容"));
        }

        Ok(())
    }

    /// 扫描一个 JSON 值并计入真实语法节点
    fn parse_value(&mut self, depth: usize) -> Result<(), InvocationKeyError> {
        self.consume_item(depth)?;
        self.skip_whitespace();

        match self.bytes.get(self.index).copied() {
            Some(b'n') => self.parse_literal(b"null"),
            Some(b't') => self.parse_literal(b"true"),
            Some(b'f') => self.parse_literal(b"false"),
            Some(b'"') => self.parse_string(),
            Some(b'[') => self.parse_array(depth),
            Some(b'{') => self.parse_object(depth),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(_) => Err(self.error("参数 JSON 包含无效值")),
            None => Err(self.error("参数 JSON 意外结束")),
        }
    }

    /// 消耗一个节点的深度与数量预算
    fn consume_item(&mut self, depth: usize) -> Result<(), InvocationKeyError> {
        if depth > self.max_depth {
            return Err(InvocationKeyError::new(format!(
                "参数 JSON 超过 {} 层深度限制",
                self.max_depth
            )));
        }

        self.item_count = self
            .item_count
            .checked_add(1)
            .ok_or_else(|| InvocationKeyError::new("参数 JSON 项目计数溢出"))?;
        if self.item_count > self.max_items {
            return Err(InvocationKeyError::new(format!(
                "参数 JSON 超过 {} 项限制",
                self.max_items
            )));
        }

        Ok(())
    }

    /// 扫描固定 JSON 字面量
    fn parse_literal(&mut self, literal: &[u8]) -> Result<(), InvocationKeyError> {
        if self.bytes.get(self.index..self.index + literal.len()) != Some(literal) {
            return Err(self.error("参数 JSON 包含无效字面量"));
        }

        self.index += literal.len();
        Ok(())
    }

    /// 扫描 JSON 字符串并校验转义序列
    fn parse_string(&mut self) -> Result<(), InvocationKeyError> {
        self.index += 1;
        while let Some(byte) = self.bytes.get(self.index).copied() {
            match byte {
                b'"' => {
                    self.index += 1;
                    return Ok(());
                }
                b'\\' => {
                    self.index += 1;
                    match self.bytes.get(self.index).copied() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.index += 1;
                        }
                        Some(b'u') => {
                            self.index += 1;
                            let Some(hex) = self.bytes.get(self.index..self.index + 4) else {
                                return Err(self.error("参数 JSON Unicode 转义不完整"));
                            };
                            if !hex.iter().all(u8::is_ascii_hexdigit) {
                                return Err(self.error("参数 JSON Unicode 转义无效"));
                            }
                            self.index += 4;
                        }
                        _ => return Err(self.error("参数 JSON 字符串转义无效")),
                    }
                }
                0x00..=0x1f => return Err(self.error("参数 JSON 字符串包含控制字符")),
                _ => self.index += 1,
            }
        }

        Err(self.error("参数 JSON 字符串未结束"))
    }

    /// 扫描 JSON 数组
    fn parse_array(&mut self, depth: usize) -> Result<(), InvocationKeyError> {
        self.index += 1;
        self.skip_whitespace();
        if self.consume_byte(b']') {
            return Ok(());
        }

        loop {
            self.parse_value(depth.saturating_add(1))?;
            self.skip_whitespace();
            if self.consume_byte(b']') {
                return Ok(());
            }
            if !self.consume_byte(b',') {
                return Err(self.error("参数 JSON 数组缺少逗号或结束符"));
            }
            self.skip_whitespace();
        }
    }

    /// 扫描 JSON 对象，属性名不计入 JSON 值节点
    fn parse_object(&mut self, depth: usize) -> Result<(), InvocationKeyError> {
        self.index += 1;
        self.skip_whitespace();
        if self.consume_byte(b'}') {
            return Ok(());
        }

        let mut property_names = HashSet::new();

        loop {
            if self.bytes.get(self.index) != Some(&b'"') {
                return Err(self.error("参数 JSON 对象属性名必须是字符串"));
            }
            let property_start = self.index;
            self.parse_string()?;
            let property_end = self.index;
            let property_name: String = serde_json::from_slice(
                self.bytes
                    .get(property_start..property_end)
                    .ok_or_else(|| self.error("参数 JSON 对象属性名范围无效"))?,
            )
            .map_err(|error| self.error(&format!("参数 JSON 对象属性名无法解析: {error}")))?;
            if !property_names.insert(property_name.clone()) {
                let display_name = if property_name.chars().count() > 128 {
                    format!("{}…", property_name.chars().take(128).collect::<String>())
                } else {
                    property_name
                };
                return Err(self.error(&format!("参数 JSON 对象包含重复属性 {display_name:?}")));
            }
            self.skip_whitespace();
            if !self.consume_byte(b':') {
                return Err(self.error("参数 JSON 对象属性名后缺少冒号"));
            }
            self.skip_whitespace();
            self.parse_value(depth.saturating_add(1))?;
            self.skip_whitespace();
            if self.consume_byte(b'}') {
                return Ok(());
            }
            if !self.consume_byte(b',') {
                return Err(self.error("参数 JSON 对象缺少逗号或结束符"));
            }
            self.skip_whitespace();
        }
    }

    /// 扫描符合 JSON 语法的数字文本
    fn parse_number(&mut self) -> Result<(), InvocationKeyError> {
        self.consume_byte(b'-');

        match self.bytes.get(self.index).copied() {
            Some(b'0') => self.index += 1,
            Some(b'1'..=b'9') => {
                self.index += 1;
                self.consume_digits();
            }
            _ => return Err(self.error("参数 JSON 数字整数部分无效")),
        }

        if self.consume_byte(b'.') {
            if !self.bytes.get(self.index).is_some_and(u8::is_ascii_digit) {
                return Err(self.error("参数 JSON 数字小数部分无效"));
            }
            self.consume_digits();
        }

        if self
            .bytes
            .get(self.index)
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            self.index += 1;
            if self
                .bytes
                .get(self.index)
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                self.index += 1;
            }
            if !self.bytes.get(self.index).is_some_and(u8::is_ascii_digit) {
                return Err(self.error("参数 JSON 数字指数部分无效"));
            }
            self.consume_digits();
        }

        Ok(())
    }

    /// 消耗连续十进制数字
    fn consume_digits(&mut self) {
        while self.bytes.get(self.index).is_some_and(u8::is_ascii_digit) {
            self.index += 1;
        }
    }

    /// 消耗指定字节
    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.bytes.get(self.index) != Some(&expected) {
            return false;
        }

        self.index += 1;
        true
    }

    /// 跳过 JSON 允许的空白字符
    fn skip_whitespace(&mut self) {
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.index += 1;
        }
    }

    /// 创建带字节偏移的 JSON 扫描错误
    fn error(&self, message: &str) -> InvocationKeyError {
        InvocationKeyError::new(format!("{message}，位置 {}", self.index))
    }
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

/// 递归规范化 JSON 对象字段并限制输入复杂度
fn canonicalize_json(
    value: serde_json::Value,
    depth: usize,
    item_count: &mut usize,
    max_depth: usize,
    max_items: usize,
) -> Result<serde_json::Value, InvocationKeyError> {
    check_json_limits(depth, item_count, max_depth, max_items)?;
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

/// 检查当前 JSON 节点是否超过深度或项目数限制
fn check_json_limits(
    depth: usize,
    item_count: &mut usize,
    max_depth: usize,
    max_items: usize,
) -> Result<(), InvocationKeyError> {
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

    Ok(())
}

/// 计算方法标识与可选参数快照组成的调用摘要
fn compose_invocation_hash(method_key: &str, arguments_key: Option<&str>) -> String {
    if method_key.chars().any(char::is_control) {
        return compose_structured_invocation_hash(method_key, arguments_key);
    }

    let mut seed = method_key.to_string();

    if let Some(arguments_key) = arguments_key {
        seed.push('\n');
        seed.push_str(arguments_key);
    }

    sha256_hex(&seed)
}

/// 对包含控制字符的方法键使用无歧义的长度前缀编码
fn compose_structured_invocation_hash(method_key: &str, arguments_key: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"source_generator_invocation_key\0");
    hasher.update(
        u64::try_from(method_key.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    hasher.update(method_key.as_bytes());

    if let Some(arguments_key) = arguments_key {
        hasher.update([1]);
        hasher.update(
            u64::try_from(arguments_key.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        hasher.update(arguments_key.as_bytes());
    } else {
        hasher.update([0]);
    }

    let hash = hasher.finalize();
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 计算输入字符串的 SHA-256 十六进制摘要
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();

    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}
