use chrono::{NaiveDate, NaiveTime};
use serde_json::Value;

use crate::{
    data_helper::DataColumnType,
    property_helper::{Property, PropertyKind, PropertyModel},
};

/// 使用属性声明顺序和名称构造参数，不借用 JSON 的重命名、跳过或排序规则。
pub fn model_to_uri_param<T: PropertyModel + ?Sized>(
    value: &T,
    url: &str,
) -> Result<String, serde_json::Error> {
    let mut parts = Vec::new();
    for property in value.properties()? {
        if property.value.is_null() {
            continue;
        }
        let text = property_text(&property)?;
        if text.trim().is_empty() {
            continue;
        }
        parts.push(format!("{}={}", encode(property.name), encode(&text)));
    }
    if parts.is_empty() {
        return Ok(url.to_owned());
    }
    let separator = if !url.contains('?') {
        "?"
    } else if url.ends_with(['?', '&']) {
        ""
    } else {
        "&"
    };
    Ok(format!("{url}{separator}{}", parts.join("&")))
}

fn property_text(property: &Property) -> Result<String, serde_json::Error> {
    Ok(match property.kind {
        PropertyKind::Enum => property.value.to_string(),
        PropertyKind::DateTime => property
            .date_time()?
            .format("%Y/%m/%d %H:%M:%S")
            .to_string(),
        PropertyKind::DateTimeOffset => property
            .offset_time()?
            .format("%Y/%m/%d %H:%M:%S %:z")
            .to_string(),
        PropertyKind::DateOnly => serde_json::from_value::<NaiveDate>(property.value.clone())?
            .format("%m/%d/%Y")
            .to_string(),
        PropertyKind::TimeOnly => serde_json::from_value::<NaiveTime>(property.value.clone())?
            .format("%H:%M")
            .to_string(),
        PropertyKind::Decimal => property.decimal()?.to_string(),
        _ => match &property.value {
            Value::String(value) => value.clone(),
            Value::Bool(value) => if *value { "True" } else { "False" }.to_owned(),
            Value::Number(value)
                if matches!(
                    property.data_type,
                    DataColumnType::Float32 | DataColumnType::Float64
                ) =>
            {
                match value.as_f64() {
                    Some(value) => float_text(value, property.data_type == DataColumnType::Float32),
                    None => value.to_string(),
                }
            }
            value => value.to_string(),
        },
    })
}

fn float_text(value: f64, single: bool) -> String {
    let (fixed, scientific, max_digits) = if single {
        let value = value as f32;
        (value.to_string(), format!("{value:E}"), 9)
    } else {
        (value.to_string(), format!("{value:E}"), 17)
    };
    let Some((mantissa, exponent)) = scientific.split_once('E') else {
        return fixed;
    };
    let Ok(exponent) = exponent.parse::<i32>() else {
        return fixed;
    };
    // 默认 G 格式使用最短可往返数字，指数范围外切换大写 E，带符号且至少两位。
    if (-4..max_digits).contains(&exponent) {
        fixed
    } else {
        format!("{mantissa}E{exponent:+03}")
    }
}

/// HttpUtility.UrlEncode：空格为 +，保留源安全字符，UTF-8 字节用小写十六进制。
fn encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"!()*-._".contains(&byte) {
            encoded.push(char::from(byte));
        } else if byte == b' ' {
            encoded.push('+');
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}
