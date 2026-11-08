//! 内部 JSON 日期转换；HTTP DTO 仍使用宿主的 ISO 输入规则。

mod iso;

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, Timelike};
use serde::{Deserialize, Deserializer, de::Error};

pub(super) const OFFSET_MARKER: &str = "NetEngine.DateTimeOffset";

/// HTTP 使用 System.Text.Json 的 ISO 语法；不复用内部日期和 Query 的宽松解析
pub fn parse_offset_iso(value: &str) -> Result<DateTime<FixedOffset>, &'static str> {
    iso::parse(value).ok_or("应提供有效的 ISO 日期时间")
}

pub fn deserialize_iso_offset_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    parse_offset_iso(&value)
        .map(|value| value.to_rfc3339())
        .map_err(D::Error::custom)
}

/// HTTP 保留显式偏移；公共对象 JSON 按日期标记选择内部格式。
pub fn serialize_offset<S: serde::Serializer>(
    value: &DateTime<FixedOffset>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let value = format_offset_iso(value).map_err(serde::ser::Error::custom)?;
    serializer.serialize_newtype_struct(OFFSET_MARKER, &value)
}

/// 显式声明为 DateTimeOffset 的字符串字段保留日期类型语义；不识别任意字符串。
pub fn serialize_offset_string<S: serde::Serializer>(
    value: &str,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let date = parse_offset(value).map_err(serde::ser::Error::custom)?;
    serialize_offset(&date, serializer)
}

pub fn serialize_optional_offset_string<S: serde::Serializer>(
    value: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    struct OffsetValue(DateTime<FixedOffset>);
    impl serde::Serialize for OffsetValue {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serialize_offset(&self.0, serializer)
        }
    }
    match value {
        Some(value) => {
            let date = parse_offset(value).map_err(serde::ser::Error::custom)?;
            serializer.serialize_some(&OffsetValue(date))
        }
        None => serializer.serialize_none(),
    }
}

/// 与 System.Text.Json 的 DateTimeOffset 输出一致：百纳秒精度、去尾零、显式偏移。
pub(super) fn format_offset_iso(value: &DateTime<FixedOffset>) -> Result<String, &'static str> {
    let offset = value.offset().local_minus_utc();
    if !(1..=9999).contains(&value.year())
        || !(1..=9999).contains(&value.naive_utc().year())
        || offset.unsigned_abs() > 14 * 3600
        || offset % 60 != 0
        || value.nanosecond() >= 1_000_000_000
    {
        return Err("日期时间超出支持范围");
    }
    let mut result = value.format("%Y-%m-%dT%H:%M:%S").to_string();
    let fraction = value.nanosecond() / 100;
    if fraction != 0 {
        result.push('.');
        result.push_str(format!("{fraction:07}").trim_end_matches('0'));
    }
    result.push_str(&value.format("%:z").to_string());
    Ok(result)
}

/// 读取 ISO、公共序列化格式及不带偏移的日期；无偏移时使用进程本地时区。
/// 这里不根据字符串内容改写普通 JSON 字段，也不推断任意区域性日期顺序。
pub fn parse_offset(value: &str) -> Result<DateTime<FixedOffset>, &'static str> {
    let value = value.trim();
    let parsed = DateTime::parse_from_rfc3339(value)
        .ok()
        .or_else(|| {
            ["%Y/%m/%d %H:%M:%S%.f %:z", "%Y-%m-%d %H:%M:%S%.f %:z"]
                .iter()
                .find_map(|format| DateTime::parse_from_str(value, format).ok())
        })
        .or_else(|| {
            let naive = [
                "%Y-%m-%dT%H:%M:%S%.f",
                "%Y-%m-%dT%H:%M",
                "%Y-%m-%d %H:%M:%S%.f",
                "%Y/%m/%d %H:%M:%S%.f",
                "%Y-%m-%d %H:%M",
                "%Y/%m/%d %H:%M",
            ]
            .iter()
            .find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
            .or_else(|| {
                ["%Y-%m-%d", "%Y/%m/%d"]
                    .iter()
                    .find_map(|format| NaiveDate::parse_from_str(value, format).ok())?
                    .and_hms_opt(0, 0, 0)
            })?;
            crate::date_time_helper::resolve_local(naive)
        })
        .ok_or("日期时间格式无效")?;
    if !(1..=9999).contains(&parsed.year())
        || !(1..=9999).contains(&parsed.naive_utc().year())
        || parsed.offset().local_minus_utc().unsigned_abs() > 14 * 3600
        || parsed.nanosecond() >= 1_000_000_000
    {
        return Err("日期时间超出支持范围");
    }
    // DateTimeOffset 使用一百纳秒精度，不接受闰秒。
    parsed
        .with_nanosecond(parsed.nanosecond() / 100 * 100)
        .ok_or("日期时间精度无效")
}

pub fn deserialize_offset<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<DateTime<FixedOffset>, D::Error> {
    let value = String::deserialize(deserializer)?;
    parse_offset(&value).map_err(D::Error::custom)
}

/// 为以字符串承载日期的内部模型保留规范 ISO 值，供后续业务转换。
pub fn deserialize_offset_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    deserialize_offset(deserializer).map(|value| value.to_rfc3339())
}
