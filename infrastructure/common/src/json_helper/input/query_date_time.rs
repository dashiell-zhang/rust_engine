//! Query 的 invariant 日期转换，与 JSON 的 ISO 转换和宿主文化排序分开。

use chrono::{
    DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, TimeDelta, TimeZone, Timelike,
    format::{Parsed, StrftimeItems},
};

pub(super) fn parse(text: &str) -> Option<DateTime<FixedOffset>> {
    let text = text.trim();
    let (body, offset) = split_offset(text)?;
    let mut body = body.to_owned();
    // DateTime.Parse 累积双精度小数后按偶数舍入至 tick；不能直接截断为微秒。
    let mut ticks = 0;
    if let Some(colon) = body.rfind(':')
        && let Some(relative) = body[colon..].find(['.', ','])
    {
        let point = colon + relative;
        if body[..point].bytes().filter(|byte| *byte == b':').count() != 2 {
            return None;
        }
        let count = body[point + 1..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        if count == 0 {
            return None;
        }
        let mut fraction = 0.0_f64;
        let mut scale = 0.1;
        for digit in body[point + 1..point + 1 + count].bytes() {
            fraction += f64::from(digit - b'0') * scale;
            scale *= 0.1;
        }
        ticks = (fraction * 10_000_000.0).round_ties_even() as i64;
        body.replace_range(point..point + 1 + count, "");
    }
    let naive = if let Some(index) = body.find(['T', 't']).filter(|index| {
        *index > 0
            && body[..*index]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'/' | b'.'))
    }) {
        // ISO 分支要求两位时分秒，不接受普通日期分支的 AM/PM 或 GMT 标记。
        if text
            .get(text.len().saturating_sub(3)..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case("GMT"))
        {
            return None;
        }
        parse_iso(&body[..index], &body[index + 1..])?
    } else {
        parse_calendar(body.trim())?
    };
    if naive.nanosecond() >= 1_000_000_000 {
        return None;
    }
    let naive = naive.checked_add_signed(TimeDelta::nanoseconds(ticks * 100))?;
    let value = match offset {
        Some(offset) => offset.from_local_datetime(&naive).single()?,
        None => crate::date_time_helper::resolve_local(naive)?,
    };
    if !(1..=9999).contains(&value.year())
        || !(1..=9999).contains(&value.naive_utc().year())
        || value.offset().local_minus_utc().unsigned_abs() > 14 * 3600
    {
        return None;
    }
    Some(value)
}

fn split_offset(text: &str) -> Option<(&str, Option<FixedOffset>)> {
    if let Some(body) = text.strip_suffix(['Z', 'z']) {
        return Some((body.trim_end(), FixedOffset::east_opt(0)));
    }
    if text
        .get(text.len().saturating_sub(3)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case("GMT"))
    {
        return Some((text[..text.len() - 3].trim_end(), FixedOffset::east_opt(0)));
    }
    let Some(index) = text.rfind(['+', '-']).filter(|index| *index > 9) else {
        return Some((text, None));
    };
    let (body, zone) = text.split_at(index);
    // 英文月份的长日期末尾也可能有减号，例如 07-November-2026。
    if zone.starts_with('-')
        && !body.contains([':', ' ', '\t'])
        && body.matches(['/', '-', '.']).count() < 2
    {
        return Some((text, None));
    }
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    let zone = &zone[1..];
    let (hours, minutes) = if let Some((hours, minutes)) = zone.split_once(':') {
        if !(1..=2).contains(&hours.len()) || minutes.len() != 2 {
            return None;
        }
        (hours, minutes)
    } else {
        match zone.len() {
            1 | 2 => (zone, "0"),
            3 | 4 => zone.split_at(zone.len() - 2),
            _ => return None,
        }
    };
    if !hours
        .bytes()
        .chain(minutes.bytes())
        .all(|digit| digit.is_ascii_digit())
    {
        return None;
    }
    let hours: i32 = hours.parse().ok()?;
    let minutes: i32 = minutes.parse().ok()?;
    if minutes > 59 || hours > 14 || hours == 14 && minutes != 0 {
        return None;
    }
    Some((
        body.trim_end(),
        Some(FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))?),
    ))
}

fn parse_iso(date: &str, time: &str) -> Option<NaiveDateTime> {
    if date.bytes().take_while(u8::is_ascii_digit).count() < 3 {
        return None;
    }
    let date = ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d"]
        .into_iter()
        .find_map(|format| NaiveDate::parse_from_str(date, format).ok())?;
    let parts: Vec<_> = time.split(':').map(str::trim).collect();
    if !(2..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|part| part.len() != 2 || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let second = parts.get(2).map_or(Some(0), |value| value.parse().ok())?;
    date.and_hms_opt(parts[0].parse().ok()?, parts[1].parse().ok()?, second)
}

fn parse_calendar(text: &str) -> Option<NaiveDateTime> {
    // invariant 的短数字日期按月/日/年解释；不能让 Chrono 将两位年份当公元 26 年。
    let date_end = text.find(char::is_whitespace).unwrap_or(text.len());
    let date = &text[..date_end];
    let parts: Vec<_> = date.split(['/', '-', '.']).collect();
    let expanded;
    let text = if parts.len() == 3
        && !parts[2].is_empty()
        && parts[2].bytes().all(|digit| digit.is_ascii_digit())
        && !(parts[0].len() >= 3 && parts[0].bytes().all(|digit| digit.is_ascii_digit()))
        && parts[2].len() <= 2
    {
        let year: u32 = parts[2].parse().ok()?;
        let year = if year <= 49 { 2000 + year } else { 1900 + year };
        let prefix = date.rsplit_once(['/', '-', '.'])?.0;
        let separator = date.as_bytes()[prefix.len()] as char;
        expanded = format!("{prefix}{separator}{year}{}", &text[date_end..]);
        expanded.as_str()
    } else {
        text
    };
    const DATES: &[&str] = &[
        "%Y-%m-%d",
        "%m/%d/%Y",
        "%Y/%m/%d",
        "%m-%d-%Y",
        "%Y.%m.%d",
        "%m.%d.%Y",
        "%d-%b-%Y",
        "%d-%B-%Y",
        "%b-%d-%Y",
        "%B-%d-%Y",
        "%Y-%b-%d",
        "%Y-%B-%d",
        "%d %b %Y",
        "%d %B %Y",
        "%b %d %Y",
        "%B %d %Y",
        "%b %d, %Y",
        "%B %d, %Y",
        "%a, %d %b %Y",
    ];
    let mark = text
        .get(text.len().saturating_sub(2)..)
        .filter(|suffix| suffix.eq_ignore_ascii_case("AM") || suffix.eq_ignore_ascii_case("PM"));
    let text = if mark.is_some() {
        text[..text.len() - 2].trim_end()
    } else {
        text
    };
    const TIMES: &[&str] = &["%H:%M:%S", "%H:%M", "%H"];
    for date in DATES {
        if mark.is_none()
            && let Ok(date) = NaiveDate::parse_from_str(text, date)
        {
            return date.and_hms_opt(0, 0, 0);
        }
        for time in TIMES {
            if *time == "%H" && mark.is_none() {
                continue;
            }
            let mut parsed = Parsed::new();
            if chrono::format::parse(
                &mut parsed,
                text,
                StrftimeItems::new(&format!("{date} {time}")),
            )
            .is_err()
            {
                continue;
            }
            if parsed.minute().is_none() {
                parsed.set_minute(0).ok()?;
            }
            if parsed.second().is_none() {
                parsed.set_second(0).ok()?;
            }
            let Ok(mut value) = parsed.to_naive_datetime_with_offset(0) else {
                continue;
            };
            if let Some(mark) = mark {
                let hour = value.hour();
                let hour = if mark.eq_ignore_ascii_case("AM") {
                    if hour > 12 {
                        return None;
                    }
                    if hour == 12 { 0 } else { hour }
                } else if hour < 12 {
                    hour + 12
                } else {
                    hour
                };
                value = value.with_hour(hour)?;
            }
            return Some(value);
        }
    }
    None
}
