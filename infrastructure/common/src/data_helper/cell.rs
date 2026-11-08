use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime};
use rust_decimal::Decimal;
use serde_json::Value;

use super::EnumSnapshot;
use crate::property_helper::{Property, PropertyKind, PropertyResult};

/// 数据表单元格保留原始类型，避免日期与 decimal 经 JSON 中转后退化为文本。
#[derive(Clone, Debug, PartialEq)]
pub enum DataCell {
    Null,
    Boolean(bool),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    Decimal(Decimal),
    DateTime(NaiveDateTime),
    DateTimeOffset(DateTime<FixedOffset>),
    DateOnly(NaiveDate),
    TimeOnly(NaiveTime),
    Enum(EnumSnapshot),
    Text(String),
    Json(Value),
}

impl DataCell {
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::Unsigned(value) => (*value).try_into().ok(),
            Self::Enum(value) => value.number_cell().as_i64(),
            _ => None,
        }
    }

    pub(crate) fn is_blank(&self) -> bool {
        matches!(self, Self::Text(value) if value.trim().is_empty())
    }

    pub(crate) fn text(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Boolean(value) => if *value { "True" } else { "False" }.to_owned(),
            Self::Integer(value) => value.to_string(),
            Self::Unsigned(value) => value.to_string(),
            Self::Float(value) if value.is_infinite() => if value.is_sign_negative() {
                "-Infinity"
            } else {
                "Infinity"
            }
            .to_owned(),
            Self::Float(value) => value.to_string(),
            Self::Decimal(value) => value.to_string(),
            Self::DateTime(value) => value.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
            Self::DateTimeOffset(value) => value.to_rfc3339(),
            Self::DateOnly(value) => value.to_string(),
            Self::TimeOnly(value) => value.to_string(),
            Self::Enum(value) => value.text().to_owned(),
            Self::Text(value) => value.clone(),
            Self::Json(value) => value.to_string(),
        }
    }

    pub(crate) fn from_property(property: Property) -> PropertyResult<Self> {
        if property.value.is_null() {
            return Ok(Self::Null);
        }
        Ok(match property.kind {
            PropertyKind::Enum => Self::Enum(property.enum_value.ok_or_else(|| {
                <serde_json::Error as serde::ser::Error>::custom("枚举属性缺少元数据")
            })?),
            PropertyKind::Decimal => Self::Decimal(property.decimal()?),
            PropertyKind::DateTime => Self::DateTime(property.date_time()?),
            PropertyKind::DateTimeOffset => Self::DateTimeOffset(property.offset_time()?),
            PropertyKind::DateOnly => Self::DateOnly(serde_json::from_value(property.value)?),
            PropertyKind::TimeOnly => Self::TimeOnly(serde_json::from_value(property.value)?),
            _ => property.value.into(),
        })
    }
}

impl From<Value> for DataCell {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Boolean(value),
            Value::String(value) => Self::Text(value),
            Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    Self::Integer(value)
                } else if let Some(value) = value.as_u64() {
                    Self::Unsigned(value)
                } else if value.to_string().contains(['.', 'e', 'E']) {
                    value
                        .as_f64()
                        .map(Self::Float)
                        .unwrap_or(Self::Json(Value::Number(value)))
                } else {
                    Self::Json(Value::Number(value))
                }
            }
            value => Self::Json(value),
        }
    }
}

impl From<&str> for DataCell {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

macro_rules! cell_from {
    ($($ty:ty => $variant:ident),* $(,)?) => {$(
        impl From<$ty> for DataCell {
            fn from(value: $ty) -> Self { Self::$variant(value) }
        }
    )*};
}
cell_from!(String => Text, bool => Boolean, i64 => Integer, u64 => Unsigned,
    f64 => Float, Decimal => Decimal, NaiveDateTime => DateTime, DateTime<FixedOffset> => DateTimeOffset,
    NaiveDate => DateOnly, NaiveTime => TimeOnly);
