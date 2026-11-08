//! 本地墙上时间到偏移日期的转换，避免各输入入口自行选择重复时间

use chrono::{DateTime, FixedOffset, Local, LocalResult, NaiveDateTime, TimeZone};
#[cfg(unix)]
use chrono::{Datelike, Timelike};

pub(crate) fn resolve_local(value: NaiveDateTime) -> Option<DateTime<FixedOffset>> {
    let offset = match Local.from_local_datetime(&value) {
        LocalResult::Single(value) => *value.offset(),
        // 新构造的本地 DateTime 没有夏令时歧义标记，源转换使用标准时偏移
        _ => standard_offset_at(value)?,
    };
    // 以原始墙上时间和所选偏移构造，不能采用 mktime 归一化后的时分秒
    offset.from_local_datetime(&value).single()
}

#[cfg(unix)]
pub(crate) fn standard_offset_at(value: NaiveDateTime) -> Option<FixedOffset> {
    // SAFETY: tm 的数值字段和指针都可零初始化；后续只向 mktime 传递同步有效的可写存储
    let mut calendar: libc::tm = unsafe { std::mem::zeroed() };
    calendar.tm_year = value.year() - 1900;
    calendar.tm_mon = value.month0() as i32;
    calendar.tm_mday = value.day() as i32;
    calendar.tm_hour = value.hour() as i32;
    calendar.tm_min = value.minute() as i32;
    calendar.tm_sec = value.second() as i32;
    calendar.tm_isdst = 0;
    calendar.tm_wday = -1;
    // SAFETY: calendar 已初始化，mktime 不保存此指针
    let timestamp = unsafe { libc::mktime(&mut calendar) };
    if timestamp == -1 {
        // -1 也可能是合法的 epoch 前一秒，不能仅凭返回值把日期判错
        // SAFETY: 输出 tm 可零初始化，localtime_r 只在本次调用中写入该存储
        let mut actual: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: timestamp 和 actual 的指针均有效，输出不与输入重叠
        if unsafe { libc::localtime_r(&timestamp, &mut actual) }.is_null()
            || calendar_fields(&calendar) != calendar_fields(&actual)
        {
            return None;
        }
    }
    let seconds = value.and_utc().timestamp().checked_sub(timestamp as i64)?;
    FixedOffset::east_opt(i32::try_from(seconds).ok()?)
}

#[cfg(unix)]
fn calendar_fields(value: &libc::tm) -> [i32; 9] {
    [
        value.tm_year,
        value.tm_mon,
        value.tm_mday,
        value.tm_hour,
        value.tm_min,
        value.tm_sec,
        value.tm_wday,
        value.tm_yday,
        value.tm_isdst,
    ]
}

#[cfg(not(unix))]
fn standard_offset_at(value: NaiveDateTime) -> Option<FixedOffset> {
    // 非 Unix 平台保持既有系统映射；逐年标准时规则另行接入对应平台 API
    Local
        .from_local_datetime(&value)
        .latest()
        .map(|value| *value.offset())
}
