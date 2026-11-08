use std::{
    any::TypeId,
    fmt::{self, Debug, Display},
    marker::PhantomData,
    ops::BitOr,
    str::FromStr,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned, ser::Error};

use super::{DataCell, DataColumnType, FromDataCell};
use crate::property_helper::PropertyResult;

mod sealed {
    pub trait Sealed {}
}

/// C# 枚举支持的八种整数底层类型；窄化转换只用于源代码的 Int32 优先分支。
pub trait EnumRepr:
    sealed::Sealed
    + Copy
    + Debug
    + Display
    + Eq
    + Default
    + BitOr<Output = Self>
    + Serialize
    + DeserializeOwned
    + 'static
{
    fn from_i32(value: i32) -> Self;
    fn parse_integer(value: &str) -> Option<Self>;
    fn bits(self) -> u64;
}

macro_rules! enum_repr {
    ($($ty:ty => $unsigned:ty),* $(,)?) => {$(
        impl sealed::Sealed for $ty {}

        impl EnumRepr for $ty {
            fn from_i32(value: i32) -> Self {
                value as Self
            }

            fn parse_integer(value: &str) -> Option<Self> {
                // i128 覆盖所有底层类型，且让无符号枚举也接受 -0。
                value.parse::<i128>().ok().and_then(|value| value.try_into().ok())
            }

            fn bits(self) -> u64 {
                (self as $unsigned) as u64
            }
        }
    )*};
}

enum_repr!(i8 => u8, u8 => u8, i16 => u16, u16 => u16,
    i32 => u32, u32 => u32, i64 => u64, u64 => u64);

/// 声明顺序用于同描述字段的首次匹配；名称独立于 Serde 重命名。
#[derive(Clone, Copy, Debug)]
pub struct EnumMember<R> {
    pub name: &'static str,
    pub description: Option<&'static str>,
    pub value: R,
}

/// 数值枚举元数据，可使用 DataEnum 派生或手动实现。
pub trait DataEnum: Copy + Debug + Eq + 'static {
    type Repr: EnumRepr;
    const MEMBERS: &'static [EnumMember<Self::Repr>];
    const FLAGS: bool = false;

    fn to_repr(self) -> Self::Repr;
    fn from_repr(value: Self::Repr) -> Option<Self>;
}

/// 枚举快照同时保存类型、数值和显示名称，描述不参与 ToString。
#[derive(Clone, Debug)]
pub struct EnumSnapshot {
    enum_type: TypeId,
    value: serde_json::Value,
    text: String,
}

impl EnumSnapshot {
    pub fn value(&self) -> &serde_json::Value {
        &self.value
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub(super) fn number_cell(&self) -> DataCell {
        self.value.clone().into()
    }
}

impl PartialEq for EnumSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.enum_type == other.enum_type && self.value == other.value
    }
}

impl Eq for EnumSnapshot {}

/// PropertyModel 的枚举字段入口；支持原生枚举、保留未知值的包装和可空字段。
pub trait EnumProperty {
    const NULLABLE: bool = false;
    fn column_type() -> DataColumnType;
    fn enum_snapshot(&self) -> PropertyResult<Option<EnumSnapshot>>;
}

impl<T: EnumProperty> EnumProperty for Option<T> {
    const NULLABLE: bool = true;
    fn column_type() -> DataColumnType {
        T::column_type()
    }

    fn enum_snapshot(&self) -> PropertyResult<Option<EnumSnapshot>> {
        self.as_ref()
            .map(EnumProperty::enum_snapshot)
            .transpose()
            .map(Option::flatten)
    }
}

impl<T: EnumProperty + ?Sized> EnumProperty for &T {
    const NULLABLE: bool = T::NULLABLE;
    fn column_type() -> DataColumnType {
        T::column_type()
    }

    fn enum_snapshot(&self) -> PropertyResult<Option<EnumSnapshot>> {
        (**self).enum_snapshot()
    }
}

/// 保留未声明数值和按位组合；禁止将任意整数直接转成 Rust 枚举。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumValue<E: DataEnum> {
    value: E::Repr,
    marker: PhantomData<E>,
}

impl<E: DataEnum> EnumValue<E> {
    pub const fn new(value: E::Repr) -> Self {
        Self {
            value,
            marker: PhantomData,
        }
    }

    pub const fn value(self) -> E::Repr {
        self.value
    }

    /// 只有准确匹配已声明值时才返回 Rust 枚举。
    pub fn known(self) -> Option<E> {
        E::from_repr(self.value)
    }
}

impl<E: DataEnum> Default for EnumValue<E> {
    fn default() -> Self {
        Self::new(E::Repr::default())
    }
}

impl<E: DataEnum> From<E> for EnumValue<E> {
    fn from(value: E) -> Self {
        Self::new(value.to_repr())
    }
}

impl<E: DataEnum> EnumProperty for EnumValue<E> {
    fn column_type() -> DataColumnType {
        DataColumnType::enumeration::<E>()
    }
    fn enum_snapshot(&self) -> PropertyResult<Option<EnumSnapshot>> {
        Ok(Some(EnumSnapshot {
            enum_type: TypeId::of::<E>(),
            value: serde_json::to_value(self.value)?,
            text: self.to_string(),
        }))
    }
}

impl<E: DataEnum> Display for EnumValue<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !E::FLAGS {
            return match E::MEMBERS.iter().find(|member| member.value == self.value) {
                Some(member) => formatter.write_str(member.name),
                None => Display::fmt(&self.value, formatter),
            };
        }
        let mut remaining = self.value.bits();
        if remaining == 0 {
            let name = E::MEMBERS
                .iter()
                .find(|member| member.value.bits() == 0)
                .map_or("0", |member| member.name);
            return formatter.write_str(name);
        }
        let mut members = E::MEMBERS.iter().collect::<Vec<_>>();
        // .NET 按相同位宽的无符号存储排序，负值不能按有符号大小排到前面。
        members.sort_by_key(|member| member.value.bits());
        if let Some(member) = members
            .iter()
            .rev()
            .find(|member| member.value == self.value)
        {
            return formatter.write_str(member.name);
        }
        let mut names = Vec::new();
        for member in members.into_iter().rev() {
            let bits = member.value.bits();
            if bits != 0 && remaining & bits == bits {
                names.push(member.name);
                remaining &= !bits;
                if remaining == 0 {
                    break;
                }
            }
        }
        if remaining != 0 {
            // 无法完全分解时输出原始整数，不输出部分名称加剩余数字。
            return Display::fmt(&self.value, formatter);
        }
        names.reverse();
        formatter.write_str(&names.join(", "))
    }
}

impl<E: DataEnum> Serialize for EnumValue<E> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl<'de, E: DataEnum> Deserialize<'de> for EnumValue<E> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        E::Repr::deserialize(deserializer).map(Self::new)
    }
}

impl<E: DataEnum> FromStr for EnumValue<E> {
    type Err = serde_json::Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        // NetEngine 先尝试 Int32，再调用 Enum.ToObject；这里有意保留截断与符号扩展。
        if let Ok(value) = text.parse::<i32>() {
            return Ok(Self::new(E::Repr::from_i32(value)));
        }
        if let Some(value) = E::Repr::parse_integer(text) {
            return Ok(Self::new(value));
        }
        // Enum.TryParse 按名称组合，不要求声明 Flags；组合中的数字不作为名称解析。
        let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
        let numeric_overflow =
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit());
        let names = if numeric_overflow {
            // Enum.TryParse 在数字溢出时不回退到名称，但外层仍会尝试描述。
            None
        } else {
            text.split(',').try_fold(E::Repr::default(), |value, name| {
                E::MEMBERS
                    .iter()
                    .find(|member| !name.trim().is_empty() && member.name == name.trim())
                    .map(|member| value | member.value)
            })
        };
        if let Some(value) = names {
            return Ok(Self::new(value));
        }
        // 仅在数字、名称均失败后匹配整段描述，不拆分描述中的逗号。
        if let Some(member) = E::MEMBERS
            .iter()
            .find(|member| member.description == Some(text))
        {
            return Ok(Self::new(member.value));
        }
        Err(serde_json::Error::custom(format!(
            "无法转换枚举：{text} => {}",
            std::any::type_name::<E>()
        )))
    }
}

impl<E: DataEnum> FromDataCell for EnumValue<E> {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if value.is_null() || value.is_blank() {
            return Ok(None);
        }
        value.text().parse().map(Some)
    }
}
