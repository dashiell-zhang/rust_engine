use std::any::type_name;

use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::{Serialize, ser::Error};
use serde_json::Value;

use crate::data_helper::{
    DataCell, DataColumnType, EnumProperty, EnumSnapshot, unwrapped_type_name,
};

pub type PropertyResult<T> = Result<T, serde_json::Error>;

/// 由字段实际类型确定，避免把日期形状的普通字符串当作日期。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyKind {
    Boolean,
    String,
    DateTime,
    DateTimeOffset,
    DateOnly,
    TimeOnly,
    Decimal,
    Number,
    Enum,
    Value,
    Reference,
}

/// 一次读取的字段快照，顺序由模型声明确定。
#[derive(Clone, Debug)]
pub struct Property {
    pub name: &'static str,
    pub display_name: Option<&'static str>,
    pub kind: PropertyKind,
    pub nullable: bool,
    pub value: Value,
    pub enum_value: Option<EnumSnapshot>,
    pub data_type: DataColumnType,
}

/// 公共模型的属性读取契约；派生实现不使用 JSON 属性名或序列化顺序。
pub trait PropertyModel {
    fn model_name() -> &'static str {
        type_name::<Self>()
            .split('<')
            .next()
            .unwrap_or(type_name::<Self>())
            .rsplit("::")
            .next()
            .unwrap_or(type_name::<Self>())
    }

    fn properties(&self) -> PropertyResult<Vec<Property>>;

    /// 区分 null 对象与没有公开属性的有效对象，用于列表转表时跳过 null。
    fn is_null_model(&self) -> bool {
        false
    }

    /// 路径每一段均按 OrdinalIgnoreCase 查找；None 表示没有这个属性。
    fn read_property(&self, path: &[&str]) -> PropertyResult<Option<Property>> {
        if path.len() != 1 {
            return Ok(None);
        }
        Ok(self
            .properties()?
            .into_iter()
            .find(|property| crate::string_helper::eq_ordinal_ignore_case(property.name, path[0])))
    }
}

impl<T: PropertyModel> PropertyModel for Option<T> {
    fn model_name() -> &'static str {
        T::model_name()
    }

    fn is_null_model(&self) -> bool {
        self.as_ref().is_none_or(PropertyModel::is_null_model)
    }

    fn properties(&self) -> PropertyResult<Vec<Property>> {
        self.as_ref()
            .map(PropertyModel::properties)
            .transpose()
            .map(Option::unwrap_or_default)
    }

    fn read_property(&self, path: &[&str]) -> PropertyResult<Option<Property>> {
        match self {
            Some(value) => value.read_property(path),
            None => Ok(Some(Property::null())),
        }
    }
}

macro_rules! property_pointer {
    ($($ty:ty),* $(,)?) => {$(
        impl<T: PropertyModel + ?Sized> PropertyModel for $ty {
            fn model_name() -> &'static str { T::model_name() }
            fn is_null_model(&self) -> bool { (**self).is_null_model() }
            fn properties(&self) -> PropertyResult<Vec<Property>> { (**self).properties() }
            fn read_property(&self, path: &[&str]) -> PropertyResult<Option<Property>> { (**self).read_property(path) }
        }
    )*};
}
property_pointer!(&T, Box<T>, std::sync::Arc<T>, std::rc::Rc<T>);

impl Property {
    /// 路径中的可空对象为 None 时不继续解引用其后续字段。
    #[doc(hidden)]
    pub fn null() -> Self {
        Self {
            name: "",
            display_name: None,
            kind: PropertyKind::Reference,
            nullable: true,
            value: Value::Null,
            enum_value: None,
            data_type: DataColumnType::Object,
        }
    }

    #[doc(hidden)]
    pub fn new<T: Serialize + ?Sized>(
        name: &'static str,
        display_name: Option<&'static str>,
        value: &T,
        value_type: bool,
    ) -> PropertyResult<Self> {
        let (kind, nullable) = property_type::<T>();
        Ok(Self {
            name,
            display_name,
            kind: if value_type && kind == PropertyKind::Reference {
                PropertyKind::Value
            } else {
                kind
            },
            nullable,
            value: serde_json::to_value(value)?,
            enum_value: None,
            data_type: DataColumnType::of::<T>(),
        })
    }

    /// 由枚举元数据读取，不依赖枚举的 JSON 名称或自定义序列化。
    #[doc(hidden)]
    pub fn new_enum<T: EnumProperty + ?Sized>(
        name: &'static str,
        display_name: Option<&'static str>,
        value: &T,
    ) -> PropertyResult<Self> {
        let enum_value = value.enum_snapshot()?;
        Ok(Self {
            name,
            display_name,
            kind: PropertyKind::Enum,
            nullable: T::NULLABLE,
            value: enum_value
                .as_ref()
                .map_or(Value::Null, |value| value.value().clone()),
            enum_value,
            data_type: T::column_type(),
        })
    }

    /// 属性字典保留实际标量类型，仅非可空 DateTime 按源规则预先转换为文本。
    pub fn scalar_cell(&self) -> PropertyResult<Option<DataCell>> {
        if self.kind == PropertyKind::Reference {
            return Ok(None);
        }
        if self.kind == PropertyKind::DateTime && !self.nullable && !self.value.is_null() {
            return self.scalar_value().map(|value| value.map(DataCell::from));
        }
        DataCell::from_property(self.clone()).map(Some)
    }

    /// 原始 JSON 标量视图；需要保留日期、decimal 和枚举类型时使用 scalar_cell。
    pub fn scalar_value(&self) -> PropertyResult<Option<Value>> {
        if self.kind == PropertyKind::Reference {
            return Ok(None);
        }
        if self.kind == PropertyKind::DateTime && !self.nullable && !self.value.is_null() {
            return Ok(Some(Value::String(
                self.date_time()?
                    .format("%Y/%m/%d %H:%M:%S%.3f")
                    .to_string(),
            )));
        }
        Ok(Some(self.value.clone()))
    }

    pub(crate) fn date_time(&self) -> PropertyResult<NaiveDateTime> {
        serde_json::from_value(self.value.clone())
    }

    pub(crate) fn offset_time(&self) -> PropertyResult<DateTime<FixedOffset>> {
        serde_json::from_value(self.value.clone())
    }

    pub(crate) fn decimal(&self) -> PropertyResult<Decimal> {
        match &self.value {
            Value::String(text) => text.parse().map_err(serde_json::Error::custom),
            Value::Number(number) => number
                .to_string()
                .parse()
                .map_err(serde_json::Error::custom),
            _ => Err(serde_json::Error::custom(
                "decimal 属性必须包含数字或数字字符串",
            )),
        }
    }

    pub(crate) fn value_equals(&self, other: &Self) -> PropertyResult<bool> {
        if self.value.is_null() || other.value.is_null() {
            return Ok(self.value == other.value);
        }
        match self.kind {
            PropertyKind::Enum => Ok(self.enum_value == other.enum_value),
            PropertyKind::Decimal => Ok(self.decimal()? == other.decimal()?),
            PropertyKind::DateTimeOffset => Ok(self.offset_time()? == other.offset_time()?),
            _ => Ok(self.value == other.value),
        }
    }

    pub(crate) fn display_value(&self) -> PropertyResult<String> {
        if self.value.is_null() {
            return Ok(String::new());
        }
        if self.kind == PropertyKind::DateTime {
            return Ok(self.date_time()?.format("%Y-%m-%d").to_string());
        }
        if self.kind == PropertyKind::Enum {
            return self
                .enum_value
                .as_ref()
                .map(|value| value.text().to_owned())
                .ok_or_else(|| serde_json::Error::custom("枚举属性缺少元数据"));
        }
        Ok(match &self.value {
            Value::Bool(value) => if *value { "是" } else { "否" }.to_string(),
            Value::String(value) => value.clone(),
            value => value.to_string(),
        })
    }
}

fn property_type<T: ?Sized>() -> (PropertyKind, bool) {
    let (name, nullable) = unwrapped_type_name::<T>();
    let kind = if name == type_name::<bool>() {
        PropertyKind::Boolean
    } else if name == type_name::<String>() || name == type_name::<str>() {
        PropertyKind::String
    } else if name == type_name::<NaiveDateTime>() {
        PropertyKind::DateTime
    } else if name == type_name::<NaiveDate>() {
        PropertyKind::DateOnly
    } else if name == type_name::<NaiveTime>() {
        PropertyKind::TimeOnly
    } else if ["i32", "i64", "u32", "u64", "f32", "f64"].contains(&name) {
        PropertyKind::Number
    } else if [
        type_name::<DateTime<Utc>>(),
        type_name::<DateTime<FixedOffset>>(),
        type_name::<DateTime<Local>>(),
        type_name::<DateTime<chrono_tz::Tz>>(),
    ]
    .contains(&name)
    {
        PropertyKind::DateTimeOffset
    } else if name == type_name::<Decimal>() {
        PropertyKind::Decimal
    } else if [
        "i8",
        "i16",
        "i128",
        "isize",
        "u8",
        "u16",
        "u128",
        "usize",
        "char",
        type_name::<uuid::Uuid>(),
    ]
    .contains(&name)
    {
        PropertyKind::Value
    } else {
        PropertyKind::Reference
    };
    (kind, nullable)
}
