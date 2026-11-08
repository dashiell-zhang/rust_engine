//! HTTP 默认日期转换的有界 ISO 读取；日期计算交给 chrono，不使用宽松格式回退

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeZone};

pub(super) fn parse(value: &str) -> Option<DateTime<FixedOffset>> {
    let bytes = value.as_bytes();
    if !(10..=42).contains(&bytes.len()) || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year = digits(&bytes[..4])? as i32;
    if year == 0 {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(year, digits(&bytes[5..7])?, digits(&bytes[8..10])?)?;
    let (mut hour, mut minute, mut second, mut nanos) = (0, 0, 0, 0);
    let mut cursor = 10;
    if bytes.len() > 10 {
        if bytes.len() < 16 || bytes[10] != b'T' || bytes[13] != b':' {
            return None;
        }
        hour = digits(&bytes[11..13])?;
        minute = digits(&bytes[14..16])?;
        cursor = 16;
        if bytes.get(cursor) == Some(&b':') {
            if bytes.len() < 19 {
                return None;
            }
            second = digits(&bytes[17..19])?;
            cursor = 19;
            if bytes.get(cursor) == Some(&b'.') {
                if bytes.len() < 21 {
                    return None;
                }
                cursor += 1;
                let count = bytes[cursor..]
                    .iter()
                    .take_while(|byte| byte.is_ascii_digit())
                    .count();
                if count > 16 {
                    return None;
                }
                // 源读取器保留前七位，后续位丢弃；小数点后直接跟偏移也沿用其读取结果
                let kept = count.min(7);
                nanos = digits(&bytes[cursor..cursor + kept])? * 10_u32.pow(9 - kept as u32);
                cursor += count;
            }
        }
    }
    let local = date.and_hms_nano_opt(hour, minute, second, nanos)?;
    let offset = match &bytes[cursor..] {
        [] => None,
        [b'Z'] => Some(FixedOffset::east_opt(0)?),
        suffix if matches!(suffix.first(), Some(b'+' | b'-')) => {
            if suffix.len() != 3 && suffix.len() != 6 {
                return None;
            }
            let hours = digits(&suffix[1..3])?;
            let minutes = if suffix.len() == 6 {
                if suffix[3] != b':' {
                    return None;
                }
                digits(&suffix[4..6])?
            } else {
                0
            };
            if hours > 14 || minutes > 59 || (hours == 14 && minutes != 0) {
                return None;
            }
            let seconds = (hours * 3600 + minutes * 60) as i32;
            Some(FixedOffset::east_opt(if suffix[0] == b'-' {
                -seconds
            } else {
                seconds
            })?)
        }
        _ => return None,
    };
    let parsed = match offset {
        Some(offset) => offset.from_local_datetime(&local).single()?,
        None => crate::date_time_helper::resolve_local(local)?,
    };
    if !(1..=9999).contains(&parsed.naive_utc().year())
        || parsed.offset().local_minus_utc().unsigned_abs() > 14 * 3600
    {
        return None;
    }
    Some(parsed)
}

fn digits(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0, |value, byte| {
        byte.is_ascii_digit()
            .then(|| value * 10 + u32::from(byte - b'0'))
    })
}
