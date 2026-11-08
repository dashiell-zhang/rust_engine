use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::{
    Decimal, RoundingStrategy,
    prelude::{FromPrimitive, ToPrimitive},
};
use serde::ser::Error;

use super::DataCell;
use crate::property_helper::PropertyResult;

/// None 表示保留既有默认值；Option<T> 可通过 Some(None) 清除可空属性。
pub trait FromDataCell: Sized {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>>;
}

/// 数据表列赋值契约。字段名精确匹配优先于显示名称，未知列返回 false。
pub trait DataModel {
    fn set_column(
        &mut self,
        column: &str,
        value: &DataCell,
        display_names: bool,
    ) -> PropertyResult<bool>;
}

fn invalid(target: &str) -> serde_json::Error {
    serde_json::Error::custom(format!("单元格无法转换为 {target}"))
}

fn skip(value: &DataCell) -> bool {
    value.is_null() || value.is_blank()
}

impl<T: FromDataCell> FromDataCell for Option<T> {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if value.is_null() {
            return Ok(None);
        }
        if value.is_blank() {
            return Ok(Some(None));
        }
        T::from_data_cell(value).map(|value| value.map(Some))
    }
}

impl FromDataCell for String {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if value.is_null() {
            return Ok(None);
        }
        if value.is_blank() {
            return Err(serde_json::Error::custom(
                "空白字符串需要可空属性，请使用 Option<String>",
            ));
        }
        Ok(Some(value.text().trim().to_owned()))
    }
}

impl FromDataCell for bool {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        let result = match value {
            DataCell::Enum(value) => return Self::from_data_cell(&value.number_cell()),
            DataCell::Boolean(value) => *value,
            DataCell::Integer(value) => *value != 0,
            DataCell::Unsigned(value) => *value != 0,
            DataCell::Float(value) => *value != 0.0,
            DataCell::Decimal(value) => !value.is_zero(),
            DataCell::Text(value) if value.trim().eq_ignore_ascii_case("true") => true,
            DataCell::Text(value) if value.trim().eq_ignore_ascii_case("false") => false,
            _ => return Err(invalid("bool")),
        };
        Ok(Some(result))
    }
}

impl FromDataCell for char {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        let result = match value {
            DataCell::Enum(value) => return Self::from_data_cell(&value.number_cell()),
            DataCell::Text(value) if value.encode_utf16().count() == 1 => value.chars().next(),
            DataCell::Integer(value) => u16::try_from(*value)
                .ok()
                .and_then(|value| char::from_u32(u32::from(value))),
            DataCell::Unsigned(value) => u16::try_from(*value)
                .ok()
                .and_then(|value| char::from_u32(u32::from(value))),
            _ => None,
        };
        result.map(Some).ok_or_else(|| invalid("char"))
    }
}

/// 当前数字文本使用点号小数与逗号分组，分组符不能出现在小数或指数部分。
fn number_text(value: &str, decimal: bool) -> PropertyResult<String> {
    let mut text = value.trim().to_owned();
    if decimal {
        if let Some(sign) = text.chars().last().filter(|ch| matches!(ch, '+' | '-')) {
            text.pop();
            if text.starts_with(['+', '-']) {
                return Err(invalid("decimal"));
            }
            if sign == '-' {
                text.insert(0, '-');
            }
        }
        if !text
            .bytes()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, b'+' | b'-' | b'.' | b','))
        {
            return Err(invalid("decimal"));
        }
    }
    if let Some(index) = text.find(',') {
        let integer_end = text.find(['.', 'e', 'E']).unwrap_or(text.len());
        if index >= integer_end || text[integer_end..].contains(',') {
            return Err(invalid("数字"));
        }
        let before_group = text[..index]
            .strip_prefix(['+', '-'])
            .unwrap_or(&text[..index]);
        if before_group.is_empty() || !before_group.bytes().all(|ch| ch.is_ascii_digit()) {
            return Err(invalid("数字"));
        }
    }
    Ok(text.replace(',', ""))
}

fn integer(value: &DataCell) -> PropertyResult<i128> {
    match value {
        DataCell::Enum(value) => integer(&value.number_cell()),
        DataCell::Boolean(value) => Ok(i128::from(*value)),
        DataCell::Integer(value) => Ok(i128::from(*value)),
        DataCell::Unsigned(value) => Ok(i128::from(*value)),
        DataCell::Float(value) => {
            let rounded = value.round_ties_even();
            // 浮点整数边界必须先检查再转换，不能依赖 Rust 饱和 cast。
            if rounded.is_finite() && rounded >= i128::MIN as f64 && rounded < -(i128::MIN as f64) {
                Ok(rounded as i128)
            } else {
                Err(invalid("整数"))
            }
        }
        DataCell::Decimal(value) => value
            .round_dp_with_strategy(0, RoundingStrategy::MidpointNearestEven)
            .to_i128()
            .ok_or_else(|| invalid("整数")),
        DataCell::Text(value) => value.trim().parse().map_err(|_| invalid("整数")),
        _ => Err(invalid("整数")),
    }
}

macro_rules! integer_cell {
    ($($ty:ty),* $(,)?) => {$(
        impl FromDataCell for $ty {
            fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
                if skip(value) { return Ok(None); }
                Self::try_from(integer(value)?).map(Some).map_err(|_| invalid(stringify!($ty)))
            }
        }
    )*};
}
integer_cell!(i8, i16, i32, i64, u8, u16, u32, u64);

impl FromDataCell for f64 {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        let result = match value {
            DataCell::Enum(value) => return Self::from_data_cell(&value.number_cell()),
            DataCell::Boolean(value) => {
                if *value {
                    1.0
                } else {
                    0.0
                }
            }
            DataCell::Integer(value) => *value as f64,
            DataCell::Unsigned(value) => *value as f64,
            DataCell::Float(value) => *value,
            DataCell::Decimal(value) => value.to_f64().ok_or_else(|| invalid("f64"))?,
            DataCell::Text(value) => number_text(value, false)?
                .parse()
                .map_err(|_| invalid("f64"))?,
            _ => return Err(invalid("f64")),
        };
        Ok(Some(result))
    }
}

impl FromDataCell for f32 {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        // 直接转换到 Single，避免先经过 Double 再舍入导致低位差异。
        let result = match value {
            DataCell::Enum(value) => return Self::from_data_cell(&value.number_cell()),
            DataCell::Boolean(value) => {
                if *value {
                    1.0
                } else {
                    0.0
                }
            }
            DataCell::Integer(value) => *value as f32,
            DataCell::Unsigned(value) => *value as f32,
            DataCell::Float(value) => *value as f32,
            DataCell::Decimal(value) => value.to_f32().ok_or_else(|| invalid("f32"))?,
            DataCell::Text(value) => number_text(value, false)?
                .parse()
                .map_err(|_| invalid("f32"))?,
            _ => return Err(invalid("f32")),
        };
        Ok(Some(result))
    }
}

impl FromDataCell for Decimal {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        let result = match value {
            DataCell::Enum(value) => return Self::from_data_cell(&value.number_cell()),
            DataCell::Boolean(value) => Decimal::from(u8::from(*value)),
            DataCell::Integer(value) => Decimal::from(*value),
            DataCell::Unsigned(value) => Decimal::from(*value),
            DataCell::Float(value) => {
                Decimal::from_f64(*value).ok_or_else(|| invalid("decimal"))?
            }
            DataCell::Decimal(value) => *value,
            DataCell::Text(value) => number_text(value, true)?
                .parse()
                .map_err(|_| invalid("decimal"))?,
            _ => return Err(invalid("decimal")),
        };
        Ok(Some(result))
    }
}

impl FromDataCell for uuid::Uuid {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        super::guid::parse(&value.text())
            .map(Some)
            .ok_or_else(|| invalid("Guid"))
    }
}

fn date(text: &str) -> Option<NaiveDate> {
    ["%Y-%m-%d", "%Y/%m/%d", "%m/%d/%Y"]
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(text, format).ok())
}

fn date_time(text: &str) -> Option<NaiveDateTime> {
    [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y/%m/%d %H:%M:%S%.f",
        "%m/%d/%Y %H:%M:%S%.f",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
    .or_else(|| date(text).map(|date| date.and_time(NaiveTime::MIN)))
}

impl FromDataCell for NaiveDateTime {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        match value {
            DataCell::DateTime(value) => Ok(Some(*value)),
            DataCell::Text(value) => date_time(value.trim())
                .map(Some)
                .ok_or_else(|| invalid("DateTime")),
            _ => Err(invalid("DateTime")),
        }
    }
}

impl FromDataCell for NaiveDate {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        match value {
            DataCell::DateOnly(value) => Ok(Some(*value)),
            DataCell::DateTime(value) => Ok(Some(value.date())),
            DataCell::Text(value) => date(value.trim())
                .map(Some)
                .ok_or_else(|| invalid("DateOnly")),
            _ => Err(invalid("DateOnly")),
        }
    }
}

impl FromDataCell for NaiveTime {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        match value {
            DataCell::TimeOnly(value) => Ok(Some(*value)),
            DataCell::DateTime(value) => Ok(Some(value.time())),
            DataCell::Text(value) => ["%H:%M:%S%.f", "%H:%M"]
                .iter()
                .find_map(|format| NaiveTime::parse_from_str(value.trim(), format).ok())
                .map(Some)
                .ok_or_else(|| invalid("TimeOnly")),
            _ => Err(invalid("TimeOnly")),
        }
    }
}

fn local_offset(value: NaiveDateTime) -> PropertyResult<DateTime<FixedOffset>> {
    crate::date_time_helper::resolve_local(value).ok_or_else(|| invalid("本地 DateTimeOffset"))
}

impl FromDataCell for DateTime<FixedOffset> {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if skip(value) {
            return Ok(None);
        }
        let result = match value {
            DataCell::DateTimeOffset(value) => *value,
            DataCell::DateTime(value) => local_offset(*value)?,
            DataCell::Text(value) => {
                let text = value.trim();
                if let Ok(value) = DateTime::parse_from_rfc3339(text) {
                    value
                } else if let Some(value) = date_time(text) {
                    local_offset(value)?
                } else {
                    return Err(invalid("DateTimeOffset"));
                }
            }
            _ => return Err(invalid("DateTimeOffset")),
        };
        Ok(Some(result))
    }
}

impl FromDataCell for DateTime<Utc> {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        DateTime::<FixedOffset>::from_data_cell(value)
            .map(|value| value.map(|value| value.with_timezone(&Utc)))
    }
}

impl FromDataCell for DateTime<Local> {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        DateTime::<FixedOffset>::from_data_cell(value)
            .map(|value| value.map(|value| value.with_timezone(&Local)))
    }
}

impl FromDataCell for serde_json::Value {
    fn from_data_cell(value: &DataCell) -> PropertyResult<Option<Self>> {
        if value.is_null() {
            return Ok(None);
        }
        match value {
            DataCell::Json(value) => Ok(Some(value.clone())),
            _ => Err(invalid("JSON 对象")),
        }
    }
}
