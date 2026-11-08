//! 按 Serde 原始类型处理 long，不能先转成 Value 后再把所有整数改为字符串。

use serde::{Serialize, Serializer, ser::*};
use std::{cell::Cell, rc::Rc};

pub(super) fn to_json<T: Serialize + ?Sized, const INTERNAL_DATES: bool>(
    value: &T,
) -> Result<String, serde_json::Error> {
    let raw_date = Rc::new(Cell::new(false));
    super::encoding::to_string::<_, true>(
        &LongValue::<_, INTERNAL_DATES, true>(value, raw_date.clone()),
        raw_date,
    )
}

/// 事件的 Web JSON 输出保留数字 long 与模型声明的 ISO 日期，不使用内部转换器。
pub(super) fn to_web_json<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    let raw_date = Rc::new(Cell::new(false));
    super::encoding::to_string::<_, false>(
        &LongValue::<_, false, false>(value, raw_date.clone()),
        raw_date,
    )
}

/// 控制器使用 long 字符串转换，但日期仍输出 ISO；不改变事件等 Web 协议的序列化。
pub(super) fn write_controller_json<
    W: std::io::Write,
    T: Serialize + ?Sized,
    const ARRAY_ITEM: bool,
>(
    writer: W,
    value: &T,
) -> Result<(), serde_json::Error> {
    let raw_date = Rc::new(Cell::new(false));
    super::encoding::to_writer_at_depth::<_, _, true>(
        writer,
        &LongValue::<_, false, true>(value, raw_date.clone()),
        raw_date,
        usize::from(ARRAY_ITEM),
    )
}

struct LongValue<'a, T: ?Sized, const INTERNAL_DATES: bool, const LONG_AS_STRING: bool>(
    &'a T,
    Rc<Cell<bool>>,
);
impl<T: Serialize + ?Sized, const INTERNAL_DATES: bool, const LONG_AS_STRING: bool> Serialize
    for LongValue<'_, T, INTERNAL_DATES, LONG_AS_STRING>
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // JSON 文档值已经丢失原始整数类型，等同于源 JsonElement，保持其自身协议。
        // 不要求 T: 'static，因此借用模型也能使用公共助手。
        let name = transparent_type_name(std::any::type_name::<T>());
        if name == std::any::type_name::<chrono::DateTime<chrono::FixedOffset>>() {
            // 对应源 DateTimeOffsetConverter。只识别原始日期类型，不猜测字符串内容。
            serialize_offset(self.0, serializer, INTERNAL_DATES, &self.1)
        } else if name == std::any::type_name::<serde_json::Value>()
            || name == std::any::type_name::<serde_json::Number>()
            || name == std::any::type_name::<serde_json::value::RawValue>()
        {
            self.0.serialize(serializer)
        } else {
            self.0
                .serialize(LongSerializer::<_, INTERNAL_DATES, LONG_AS_STRING>(
                    serializer,
                    self.1.clone(),
                ))
        }
    }
}

/// Serde 对这些指针直接调用内部值，不经过 serialize_newtype_struct。
fn transparent_type_name(mut name: &str) -> &str {
    let pointers = [
        std::any::type_name::<Box<()>>(),
        std::any::type_name::<std::sync::Arc<()>>(),
        std::any::type_name::<std::rc::Rc<()>>(),
        std::any::type_name::<std::borrow::Cow<'_, ()>>(),
    ];
    loop {
        if let Some(inner) = name.strip_prefix('&') {
            name = inner.strip_prefix("mut ").unwrap_or(inner);
            continue;
        }
        let inner = pointers.iter().find_map(|pointer| {
            let prefix = pointer.strip_suffix("()>")?;
            name.strip_prefix(prefix)?.strip_suffix('>')
        });
        match inner {
            Some(inner) => name = inner,
            None => return name,
        }
    }
}

struct LongSerializer<S, const INTERNAL_DATES: bool, const LONG_AS_STRING: bool>(S, Rc<Cell<bool>>);

fn serialize_offset<T: Serialize + ?Sized, S: Serializer>(
    value: &T,
    serializer: S,
    internal_dates: bool,
    raw_date: &Cell<bool>,
) -> Result<S::Ok, S::Error> {
    let value = serde_json::to_value(value).map_err(S::Error::custom)?;
    let text = value
        .as_str()
        .ok_or_else(|| S::Error::custom("日期序列化值无效"))?;
    let date = chrono::DateTime::parse_from_rfc3339(text).map_err(S::Error::custom)?;
    if internal_dates {
        serializer.collect_str(&date.format("%Y/%m/%d %H:%M:%S %:z"))
    } else {
        let value = super::date_time::format_offset_iso(&date).map_err(S::Error::custom)?;
        // 只作用于本次同步序列化，普通字符串与 JsonElement 不能绕过编码器。
        struct Restore<'a>(&'a Cell<bool>, bool);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _scope = Restore(raw_date, raw_date.replace(true));
        serializer.serialize_str(&value)
    }
}

macro_rules! scalar {
    ($($method:ident($ty:ty)),* $(,)?) => {$(
        fn $method(self, value: $ty) -> Result<Self::Ok, Self::Error> {
            self.0.$method(value)
        }
    )*};
}

impl<S: Serializer, const INTERNAL_DATES: bool, const LONG_AS_STRING: bool> Serializer
    for LongSerializer<S, INTERNAL_DATES, LONG_AS_STRING>
{
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Compound<S::SerializeSeq, INTERNAL_DATES, LONG_AS_STRING>;
    type SerializeTuple = Compound<S::SerializeTuple, INTERNAL_DATES, LONG_AS_STRING>;
    type SerializeTupleStruct = Compound<S::SerializeTupleStruct, INTERNAL_DATES, LONG_AS_STRING>;
    type SerializeTupleVariant = Compound<S::SerializeTupleVariant, INTERNAL_DATES, LONG_AS_STRING>;
    type SerializeMap = Compound<S::SerializeMap, INTERNAL_DATES, LONG_AS_STRING>;
    type SerializeStruct = Compound<S::SerializeStruct, INTERNAL_DATES, LONG_AS_STRING>;
    type SerializeStructVariant =
        Compound<S::SerializeStructVariant, INTERNAL_DATES, LONG_AS_STRING>;

    scalar!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i128(i128),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_u128(u128),
        serialize_char(char),
        serialize_str(&str),
        serialize_bytes(&[u8])
    );

    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        if LONG_AS_STRING {
            self.0.collect_str(&value)
        } else {
            self.0.serialize_i64(value)
        }
    }
    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        if !value.is_finite() {
            return Err(Self::Error::custom("JSON 不支持非有限浮点数"));
        }
        self.0.serialize_f32(value)
    }
    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        if !value.is_finite() {
            return Err(Self::Error::custom("JSON 不支持非有限浮点数"));
        }
        self.0.serialize_f64(value)
    }
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_none()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        self.0
            .serialize_some(&LongValue::<_, INTERNAL_DATES, LONG_AS_STRING>(
                value,
                self.1.clone(),
            ))
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit()
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_struct(name)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_unit_variant(name, index, variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        if name == super::date_time::OFFSET_MARKER {
            serialize_offset(value, self.0, INTERNAL_DATES, &self.1)
        } else {
            self.0.serialize_newtype_struct(
                name,
                &LongValue::<_, INTERNAL_DATES, LONG_AS_STRING>(value, self.1.clone()),
            )
        }
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.serialize_newtype_variant(
            name,
            index,
            variant,
            &LongValue::<_, INTERNAL_DATES, LONG_AS_STRING>(value, self.1.clone()),
        )
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.0
            .serialize_seq(len)
            .map(|inner| Compound::<_, INTERNAL_DATES, LONG_AS_STRING>(inner, self.1))
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.0
            .serialize_tuple(len)
            .map(|inner| Compound::<_, INTERNAL_DATES, LONG_AS_STRING>(inner, self.1))
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.0
            .serialize_tuple_struct(name, len)
            .map(|inner| Compound::<_, INTERNAL_DATES, LONG_AS_STRING>(inner, self.1))
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.0
            .serialize_tuple_variant(name, index, variant, len)
            .map(|inner| Compound::<_, INTERNAL_DATES, LONG_AS_STRING>(inner, self.1))
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        self.0
            .serialize_map(len)
            .map(|inner| Compound::<_, INTERNAL_DATES, LONG_AS_STRING>(inner, self.1))
    }
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.0
            .serialize_struct(name, len)
            .map(|inner| Compound::<_, INTERNAL_DATES, LONG_AS_STRING>(inner, self.1))
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.0
            .serialize_struct_variant(name, index, variant, len)
            .map(|inner| Compound::<_, INTERNAL_DATES, LONG_AS_STRING>(inner, self.1))
    }
    fn collect_str<T: std::fmt::Display + ?Sized>(
        self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.0.collect_str(value)
    }
    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

struct Compound<S, const INTERNAL_DATES: bool, const LONG_AS_STRING: bool>(S, Rc<Cell<bool>>);

macro_rules! sequence {
    ($trait:ident, $method:ident) => {
        impl<S: $trait, const INTERNAL_DATES: bool, const LONG_AS_STRING: bool> $trait
            for Compound<S, INTERNAL_DATES, LONG_AS_STRING>
        {
            type Ok = S::Ok;
            type Error = S::Error;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
                self.0
                    .$method(&LongValue::<_, INTERNAL_DATES, LONG_AS_STRING>(
                        value,
                        self.1.clone(),
                    ))
            }
            fn end(self) -> Result<Self::Ok, Self::Error> {
                self.0.end()
            }
        }
    };
}
sequence!(SerializeSeq, serialize_element);
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);

impl<S: SerializeMap, const INTERNAL_DATES: bool, const LONG_AS_STRING: bool> SerializeMap
    for Compound<S, INTERNAL_DATES, LONG_AS_STRING>
{
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        // 字典键由目标 JSON 序列化器处理，不应用模型字段或 long 值转换。
        self.0.serialize_key(key)
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0
            .serialize_value(&LongValue::<_, INTERNAL_DATES, LONG_AS_STRING>(
                value,
                self.1.clone(),
            ))
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.0.end()
    }
}

macro_rules! structure {
    ($trait:ident) => {
        impl<S: $trait, const INTERNAL_DATES: bool, const LONG_AS_STRING: bool> $trait
            for Compound<S, INTERNAL_DATES, LONG_AS_STRING>
        {
            type Ok = S::Ok;
            type Error = S::Error;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), Self::Error> {
                self.0.serialize_field(
                    key,
                    &LongValue::<_, INTERNAL_DATES, LONG_AS_STRING>(value, self.1.clone()),
                )
            }
            fn skip_field(&mut self, key: &'static str) -> Result<(), Self::Error> {
                self.0.skip_field(key)
            }
            fn end(self) -> Result<Self::Ok, Self::Error> {
                self.0.end()
            }
        }
    };
}
structure!(SerializeStruct);
structure!(SerializeStructVariant);
