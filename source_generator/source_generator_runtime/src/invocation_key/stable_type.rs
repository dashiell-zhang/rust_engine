use crate::*;

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
    Duration => "rust:duration",
    serde_json::Number => "serde-json:number",
    serde_json::Value => "serde-json:value",
    uuid::Uuid => "uuid:uuid",
    std::collections::hash_map::RandomState => "rust:random-state",
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
impl_stable_key_wrapper!(Vec, "rust:vec");
impl_stable_key_wrapper!(VecDeque, "rust:vec-deque");
impl_stable_key_wrapper!(LinkedList, "rust:linked-list");
impl_stable_key_wrapper!(BTreeSet, "rust:btree-set");

macro_rules! impl_stable_key_unsized_wrapper {
    ($type:ident, $tag:literal) => {
        impl<T> StableKeyType for $type<T>
        where
            T: StableKeyType + ?Sized,
        {
            fn stable_key_type() -> String {
                compose_stable_key_type($tag, &[T::stable_key_type()])
            }
        }
    };
}

impl_stable_key_unsized_wrapper!(Box, "rust:box");
impl_stable_key_unsized_wrapper!(Rc, "rust:rc");
impl_stable_key_unsized_wrapper!(Arc, "rust:arc");
impl_stable_key_unsized_wrapper!(PhantomData, "rust:phantom-data");

impl<I> StableKeyType for UnorderedKey<I>
where
    I: StableKeyType,
{
    /// 返回无序参数键包装的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type("rust:unordered-key", &[I::stable_key_type()])
    }
}

impl<T> StableKeyType for TrustedSerdeKey<T>
where
    T: StableKeyType,
{
    /// 返回显式信任 Serde 键包装的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type("rust:trusted-serde-key", &[T::stable_key_type()])
    }
}

impl<T, S> StableKeyType for HashSet<T, S>
where
    T: StableKeyType,
    S: BuildHasher + StableKeyType,
{
    /// 返回哈希集合的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type(
            "rust:hash-set",
            &[T::stable_key_type(), S::stable_key_type()],
        )
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
    S: BuildHasher + StableKeyType,
{
    /// 返回哈希字典的稳定类型标签
    fn stable_key_type() -> String {
        compose_stable_key_type(
            "rust:hash-map",
            &[
                K::stable_key_type(),
                V::stable_key_type(),
                S::stable_key_type(),
            ],
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
impl_stable_key_tuple!("rust:tuple9"; T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_stable_key_tuple!("rust:tuple10"; T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_stable_key_tuple!("rust:tuple11"; T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_stable_key_tuple!("rust:tuple12"; T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);
impl_stable_key_tuple!("rust:tuple13"; T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13);
impl_stable_key_tuple!("rust:tuple14"; T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14);
impl_stable_key_tuple!("rust:tuple15"; T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15);
impl_stable_key_tuple!("rust:tuple16"; T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16);

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
