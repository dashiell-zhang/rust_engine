use std::{any::type_name, collections::HashSet, slice};

use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::ser::Error;

use super::{DataCell, DataEnum, DataTableRow};
#[cfg(feature = "runtime")]
use crate::property_helper::Property;
use crate::property_helper::PropertyResult;

/// 来自声明而非首行值的列类型；可空属性去除 Option 后记录底层类型。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataColumnType {
    Object,
    Boolean,
    String,
    Int8,
    UInt8,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float32,
    Float64,
    Decimal,
    Char,
    Guid,
    DateTime,
    DateTimeOffset,
    DateOnly,
    TimeOnly,
    Enum {
        name: &'static str,
        underlying: &'static str,
    },
    Custom(&'static str),
}

impl DataColumnType {
    pub fn of<T: ?Sized>() -> Self {
        let (name, _) = unwrapped_type_name::<T>();
        match name {
            "bool" => Self::Boolean,
            "str" => Self::String,
            "i8" => Self::Int8,
            "u8" => Self::UInt8,
            "i16" => Self::Int16,
            "u16" => Self::UInt16,
            "i32" => Self::Int32,
            "u32" => Self::UInt32,
            "i64" => Self::Int64,
            "u64" => Self::UInt64,
            "f32" => Self::Float32,
            "f64" => Self::Float64,
            "char" => Self::Char,
            name if name == type_name::<String>() => Self::String,
            name if name == type_name::<Decimal>() => Self::Decimal,
            name if name == type_name::<uuid::Uuid>() => Self::Guid,
            name if name == type_name::<NaiveDateTime>() => Self::DateTime,
            name if name == type_name::<NaiveDate>() => Self::DateOnly,
            name if name == type_name::<NaiveTime>() => Self::TimeOnly,
            name if [
                type_name::<DateTime<Utc>>(),
                type_name::<DateTime<Local>>(),
                type_name::<DateTime<FixedOffset>>(),
                type_name::<DateTime<chrono_tz::Tz>>(),
            ]
            .contains(&name) =>
            {
                Self::DateTimeOffset
            }
            name if name == type_name::<serde_json::Value>() => Self::Object,
            name => Self::Custom(name),
        }
    }

    pub fn enumeration<E: DataEnum>() -> Self {
        Self::Enum {
            name: type_name::<E>(),
            underlying: type_name::<E::Repr>(),
        }
    }
}

/// DataColumn 默认允许 DBNull，即使源属性是非可空值类型。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataColumn {
    pub name: String,
    pub data_type: DataColumnType,
    pub allow_null: bool,
}

impl DataColumn {
    pub fn new(name: impl Into<String>, data_type: DataColumnType) -> Self {
        Self {
            name: name.into(),
            data_type,
            allow_null: true,
        }
    }

    #[cfg(feature = "runtime")]
    pub(super) fn from_property(property: &Property) -> Self {
        Self::new(property.name, property.data_type.clone())
    }
}

/// 表结构与数据行独立保存，空表不会丢失列名或类型。
#[derive(Clone, Debug, Default)]
pub struct DataTable {
    name: String,
    columns: Vec<DataColumn>,
    rows: Vec<DataTableRow>,
}

impl DataTable {
    pub fn new(name: impl Into<String>, columns: Vec<DataColumn>) -> PropertyResult<Self> {
        let mut names = HashSet::new();
        for column in &columns {
            if column.name.is_empty() || !names.insert(column.name.as_str()) {
                return Err(serde_json::Error::custom(format!(
                    "数据表列名为空或重复：{}",
                    column.name
                )));
            }
        }
        Ok(Self {
            name: name.into(),
            columns,
            rows: Vec::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn columns(&self) -> &[DataColumn] {
        &self.columns
    }
    pub fn rows(&self) -> &[DataTableRow] {
        &self.rows
    }
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn iter(&self) -> slice::Iter<'_, DataTableRow> {
        self.rows.iter()
    }

    /// 按列顺序保存，缺失值补 DBNull；失败时不向表中提交半行。
    pub fn push_row(&mut self, mut row: DataTableRow) -> PropertyResult<()> {
        let mut ordered = DataTableRow::with_capacity(self.columns.len());
        for column in &self.columns {
            let value = row.swap_remove(&column.name).unwrap_or(DataCell::Null);
            if value.is_null() && !column.allow_null {
                return Err(serde_json::Error::custom(format!(
                    "数据表列不允许空值：{}",
                    column.name
                )));
            }
            ordered.insert(column.name.clone(), value);
        }
        if let Some(name) = row.keys().next() {
            return Err(serde_json::Error::custom(format!("数据表列不存在：{name}")));
        }
        self.rows.push(ordered);
        Ok(())
    }
}

impl<'a> IntoIterator for &'a DataTable {
    type Item = &'a DataTableRow;
    type IntoIter = slice::Iter<'a, DataTableRow>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl IntoIterator for DataTable {
    type Item = DataTableRow;
    type IntoIter = std::vec::IntoIter<DataTableRow>;
    fn into_iter(self) -> Self::IntoIter {
        self.rows.into_iter()
    }
}

pub(crate) fn unwrapped_type_name<T: ?Sized>() -> (&'static str, bool) {
    let mut name = type_name::<T>();
    let mut nullable = false;
    loop {
        if let Some(inner) = name.strip_prefix('&') {
            name = inner.strip_prefix("mut ").unwrap_or(inner);
        } else if let Some(inner) = name
            .strip_prefix("core::option::Option<")
            .and_then(|name| name.strip_suffix('>'))
        {
            name = inner;
            nullable = true;
        } else {
            return (name, nullable);
        }
    }
}
