use crate::*;

use super::canonical_json::*;

/// 参数类型包含序列化时可能阻塞的同步锁
pub(crate) const BLOCKING_LOCK_PARAMETER_ERROR: &str = "参数包含序列化时可能阻塞的同步锁类型";

/// 缓存参数类型包含可变引用或可变裸指针
pub(crate) const MUTABLE_CACHE_PARAMETER_ERROR: &str = "缓存参数包含可变引用或可变裸指针";

/// 参数类型包含无法生成稳定键的 trait object
pub(crate) const TRAIT_OBJECT_PARAMETER_ERROR: &str = "参数包含无法生成稳定键的 trait object";

/// 为实现稳定键契约的值生成参数键
pub fn arguments_key<T>(args: &T) -> Result<String, InvocationKeyError>
where
    T: StableInvocationKey + ?Sized,
{
    let mut writer = StableInvocationKeyWriter::new();
    args.write_stable_invocation_key(&mut writer)?;
    writer.finish()
}

macro_rules! impl_stable_invocation_scalar {
    ($($type:ty => $tag:literal),+ $(,)?) => {
        $(
            impl StableInvocationKey for $type {
                fn write_stable_invocation_key(&self, writer: &mut StableInvocationKeyWriter) -> Result<(), InvocationKeyError> {
                    writer.write_scalar($tag, &self.to_string())
                }
            }
        )+
    };
}

impl_stable_invocation_scalar!(
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
);

impl StableInvocationKey for () {
    /// 写入单元值稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        writer.write_node("rust:unit", 0, |_| Ok(()))
    }
}

impl StableInvocationKey for str {
    /// 写入字符串切片稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        writer.write_scalar("rust:str", self)
    }
}

impl StableInvocationKey for String {
    /// 写入字符串稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        self.as_str().write_stable_invocation_key(writer)
    }
}

impl StableInvocationKey for f32 {
    /// 写入有限 f32 的稳定二进制表示
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        if !self.is_finite() {
            return Err(InvocationKeyError::new("稳定调用键不支持非有限 f32"));
        }
        let value = if *self == 0.0 { 0.0 } else { *self };
        writer.write_scalar("rust:f32", &format!("{:08x}", value.to_bits()))
    }
}

impl StableInvocationKey for f64 {
    /// 写入有限 f64 的稳定二进制表示
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        if !self.is_finite() {
            return Err(InvocationKeyError::new("稳定调用键不支持非有限 f64"));
        }
        let value = if *self == 0.0 { 0.0 } else { *self };
        writer.write_scalar("rust:f64", &format!("{:016x}", value.to_bits()))
    }
}

impl StableInvocationKey for uuid::Uuid {
    /// 写入 UUID 稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        writer.write_scalar("uuid:uuid", &self.hyphenated().to_string())
    }
}

impl StableInvocationKey for serde_json::Number {
    /// 写入任意精度 JSON 数字稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        writer.write_scalar("serde-json:number", &self.to_string())
    }
}

impl StableInvocationKey for serde_json::Value {
    /// 写入完整保留 JSON 类型与结构的稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        writer.write_node("serde-json:value", 1, |writer| match self {
            serde_json::Value::Null => writer.write_node("serde-json:null", 0, |_| Ok(())),
            serde_json::Value::Bool(value) => {
                writer.write_scalar("serde-json:bool", &value.to_string())
            }
            serde_json::Value::Number(value) => value.write_stable_invocation_key(writer),
            serde_json::Value::String(value) => writer.write_scalar("serde-json:string", value),
            serde_json::Value::Array(values) => {
                writer.write_node("serde-json:array", values.len(), |writer| {
                    for value in values {
                        value.write_stable_invocation_key(writer)?;
                    }
                    Ok(())
                })
            }
            serde_json::Value::Object(values) => {
                let mut entries = values.iter().collect::<Vec<_>>();
                entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
                writer.write_node("serde-json:object", entries.len(), |writer| {
                    for (key, value) in entries {
                        writer.write_node("serde-json:property", 2, |writer| {
                            writer.write_scalar("serde-json:property-name", key)?;
                            value.write_stable_invocation_key(writer)
                        })?;
                    }
                    Ok(())
                })
            }
        })
    }
}

impl StableInvocationKey for Duration {
    /// 写入时长稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        writer.write_node("rust:duration", 2, |writer| {
            self.as_secs().write_stable_invocation_key(writer)?;
            self.subsec_nanos().write_stable_invocation_key(writer)
        })
    }
}

impl<T> StableInvocationKey for &T
where
    T: StableInvocationKey + ?Sized,
{
    /// 透传不可变引用的稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        (*self).write_stable_invocation_key(writer)
    }
}

macro_rules! impl_stable_invocation_wrapper {
    ($type:ident) => {
        impl<T> StableInvocationKey for $type<T>
        where
            T: StableInvocationKey + ?Sized,
        {
            fn write_stable_invocation_key(
                &self,
                writer: &mut StableInvocationKeyWriter,
            ) -> Result<(), InvocationKeyError> {
                self.as_ref().write_stable_invocation_key(writer)
            }
        }
    };
}

impl_stable_invocation_wrapper!(Box);
impl_stable_invocation_wrapper!(Rc);
impl_stable_invocation_wrapper!(Arc);

impl<T> StableInvocationKey for Option<T>
where
    T: StableInvocationKey,
{
    /// 写入可选值稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        match self {
            Some(value) => writer.write_node("rust:option:some", 1, |writer| {
                value.write_stable_invocation_key(writer)
            }),
            None => writer.write_node("rust:option:none", 0, |_| Ok(())),
        }
    }
}

impl<T, E> StableInvocationKey for Result<T, E>
where
    T: StableInvocationKey,
    E: StableInvocationKey,
{
    /// 写入结果值稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        match self {
            Ok(value) => writer.write_node("rust:result:ok", 1, |writer| {
                value.write_stable_invocation_key(writer)
            }),
            Err(error) => writer.write_node("rust:result:err", 1, |writer| {
                error.write_stable_invocation_key(writer)
            }),
        }
    }
}

/// 为有序集合写入稳定键
pub(crate) fn write_ordered_values<'a, T, I>(
    writer: &mut StableInvocationKeyWriter,
    tag: &str,
    values: I,
    count: usize,
) -> Result<(), InvocationKeyError>
where
    T: StableInvocationKey + 'a,
    I: IntoIterator<Item = &'a T>,
{
    writer.write_node(tag, count, |writer| {
        for value in values {
            value.write_stable_invocation_key(writer)?;
        }
        Ok(())
    })
}

impl<T> StableInvocationKey for [T]
where
    T: StableInvocationKey,
{
    /// 写入切片稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        write_ordered_values(writer, "rust:slice", self.iter(), self.len())
    }
}

impl<T, const N: usize> StableInvocationKey for [T; N]
where
    T: StableInvocationKey,
{
    /// 写入数组稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        writer.write_node("rust:array", N.saturating_add(1), |writer| {
            N.write_stable_invocation_key(writer)?;
            for value in self {
                value.write_stable_invocation_key(writer)?;
            }
            Ok(())
        })
    }
}

macro_rules! impl_stable_invocation_sequence {
    ($type:ident, $tag:literal) => {
        impl<T> StableInvocationKey for $type<T>
        where
            T: StableInvocationKey,
        {
            fn write_stable_invocation_key(
                &self,
                writer: &mut StableInvocationKeyWriter,
            ) -> Result<(), InvocationKeyError> {
                write_ordered_values(writer, $tag, self.iter(), self.len())
            }
        }
    };
}

impl_stable_invocation_sequence!(Vec, "rust:vec");
impl_stable_invocation_sequence!(VecDeque, "rust:vec-deque");
impl_stable_invocation_sequence!(LinkedList, "rust:linked-list");
impl_stable_invocation_sequence!(BTreeSet, "rust:btree-set");

impl<K, V> StableInvocationKey for BTreeMap<K, V>
where
    K: StableInvocationKey,
    V: StableInvocationKey,
{
    /// 按键顺序写入有序字典稳定键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        writer.write_node("rust:btree-map", self.len(), |writer| {
            for (key, value) in self {
                writer.write_node("rust:map-entry", 2, |writer| {
                    key.write_stable_invocation_key(writer)?;
                    value.write_stable_invocation_key(writer)
                })?;
            }
            Ok(())
        })
    }
}

impl<T> StableInvocationKey for PhantomData<T>
where
    T: StableKeyType + ?Sized,
{
    /// 写入幽灵类型的稳定标签
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        writer.write_scalar("rust:phantom-data", &T::stable_key_type())
    }
}

macro_rules! impl_stable_invocation_tuple {
    ($tag:literal; $($type:ident => $index:tt),+) => {
        impl<$($type),+> StableInvocationKey for ($($type,)+)
        where
            $($type: StableInvocationKey),+
        {
            fn write_stable_invocation_key(&self, writer: &mut StableInvocationKeyWriter) -> Result<(), InvocationKeyError> {
                writer.write_node($tag, impl_stable_invocation_tuple!(@count $($type),+), |writer| {
                    $(self.$index.write_stable_invocation_key(writer)?;)+
                    Ok(())
                })
            }
        }
    };
    (@count $($type:ident),+) => {
        <[()]>::len(&[$(impl_stable_invocation_tuple!(@unit $type)),+])
    };
    (@unit $type:ident) => { () };
}

impl_stable_invocation_tuple!("rust:tuple1"; T1 => 0);
impl_stable_invocation_tuple!("rust:tuple2"; T1 => 0, T2 => 1);
impl_stable_invocation_tuple!("rust:tuple3"; T1 => 0, T2 => 1, T3 => 2);
impl_stable_invocation_tuple!("rust:tuple4"; T1 => 0, T2 => 1, T3 => 2, T4 => 3);
impl_stable_invocation_tuple!("rust:tuple5"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4);
impl_stable_invocation_tuple!("rust:tuple6"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5);
impl_stable_invocation_tuple!("rust:tuple7"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6);
impl_stable_invocation_tuple!("rust:tuple8"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6, T8 => 7);
impl_stable_invocation_tuple!("rust:tuple9"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6, T8 => 7, T9 => 8);
impl_stable_invocation_tuple!("rust:tuple10"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6, T8 => 7, T9 => 8, T10 => 9);
impl_stable_invocation_tuple!("rust:tuple11"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6, T8 => 7, T9 => 8, T10 => 9, T11 => 10);
impl_stable_invocation_tuple!("rust:tuple12"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6, T8 => 7, T9 => 8, T10 => 9, T11 => 10, T12 => 11);
impl_stable_invocation_tuple!("rust:tuple13"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6, T8 => 7, T9 => 8, T10 => 9, T11 => 10, T12 => 11, T13 => 12);
impl_stable_invocation_tuple!("rust:tuple14"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6, T8 => 7, T9 => 8, T10 => 9, T11 => 10, T12 => 11, T13 => 12, T14 => 13);
impl_stable_invocation_tuple!("rust:tuple15"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6, T8 => 7, T9 => 8, T10 => 9, T11 => 10, T12 => 11, T13 => 12, T14 => 13, T15 => 14);
impl_stable_invocation_tuple!("rust:tuple16"; T1 => 0, T2 => 1, T3 => 2, T4 => 3, T5 => 4, T6 => 5, T7 => 6, T8 => 7, T9 => 8, T10 => 9, T11 => 10, T12 => 11, T13 => 12, T14 => 13, T15 => 14, T16 => 15);

/// 将无序集合显式包装为按元素稳定键排序的参数键值
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

            let key = serialize_arguments_key(&value, ParameterKeySafetyPolicy::default())
                .map_err(S::Error::custom)?;
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

impl<I> StableInvocationKey for UnorderedKey<I>
where
    I: Clone + IntoIterator,
    I::Item: StableInvocationKey,
{
    /// 按元素稳定键排序后写入无序集合键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        const MAX_ITEMS: usize = 10_000;
        const MAX_BYTES: usize = 1024 * 1024;

        let mut values = Vec::new();
        let mut total_bytes = 0usize;
        for value in self.values.clone() {
            if values.len() >= MAX_ITEMS {
                return Err(InvocationKeyError::new(format!(
                    "无序参数集合超过 {MAX_ITEMS} 项限制"
                )));
            }
            let value = arguments_key(&value)?;
            total_bytes = total_bytes
                .checked_add(value.len())
                .filter(|length| *length <= MAX_BYTES)
                .ok_or_else(|| {
                    InvocationKeyError::new(format!("无序参数集合超过 {MAX_BYTES} 字节限制"))
                })?;
            values.push(value);
        }
        values.sort_unstable();

        writer.write_node("rust:unordered", values.len(), |writer| {
            for value in values {
                writer.write_scalar("rust:unordered-item", &value)?;
            }
            Ok(())
        })
    }
}

/// 显式确认 Serde 表示完整且稳定的参数键包装
pub struct TrustedSerdeKey<T> {
    value: T,
}

/// 创建由调用方保证完整性的 Serde 参数键
pub fn trusted_serde_key<T>(value: T) -> TrustedSerdeKey<T> {
    TrustedSerdeKey { value }
}

impl<T> StableInvocationKey for TrustedSerdeKey<T>
where
    T: Serialize,
{
    /// 将显式信任的 Serde 表示写入稳定调用键
    fn write_stable_invocation_key(
        &self,
        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError> {
        let value = serialize_arguments_key(&self.value, ParameterKeySafetyPolicy::default())?;
        writer.write_scalar("trusted-serde-json", &value)
    }
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

/// 将宏生成的参数值编码为有边界的稳定调用键
#[doc(hidden)]
pub fn invocation_arguments_key<T>(
    args: &InvocationArguments<T>,
) -> Result<String, InvocationKeyError>
where
    T: StableInvocationKey,
{
    arguments_key(&args.0)
}

/// 将宏生成的缓存参数元组编码为有边界的稳定调用键
#[doc(hidden)]
pub fn cacheable_invocation_arguments_key<T>(
    args: &InvocationArguments<T>,
) -> Result<String, InvocationKeyError>
where
    T: StableInvocationKey,
{
    arguments_key(&args.0)
}

/// 按指定参数语义生成规范 JSON
pub(crate) fn serialize_arguments_key<T>(
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
pub(crate) struct ParameterKeySafetyPolicy {
    reject_blocking_locks: bool,
    reject_mutable_types: bool,
    reject_trait_objects: bool,
}

impl ParameterKeySafetyPolicy {
    /// 创建缓存方法原始参数使用的类型安全策略
    pub(crate) fn cacheable_parameter() -> Self {
        Self {
            reject_blocking_locks: false,
            reject_mutable_types: true,
            reject_trait_objects: false,
        }
    }

    /// 检查指定参数类型是否可以安全进入同步序列化流程
    pub(crate) fn validate<T>(self) -> Result<(), &'static str>
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
pub(crate) fn contains_runtime_mutable_type(type_name: &str) -> bool {
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
pub(crate) fn contains_runtime_trait_object(type_name: &str) -> bool {
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
pub(crate) fn contains_runtime_blocking_lock(type_name: &str) -> bool {
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
