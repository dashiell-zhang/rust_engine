//! 任务配置和调度共用的 Quartz 风格 Cron：秒 分 时 日 月 星期 [年]。
use chrono::{
    DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike,
    Utc,
};
use std::{collections::BTreeSet, fmt, str::FromStr, sync::OnceLock};

const SECOND: usize = 0;
const MINUTE: usize = 1;
const HOUR: usize = 2;
const DAY: usize = 3;
const MONTH: usize = 4;
const WEEKDAY: usize = 5;
const YEAR: usize = 6;
const UNSPECIFIED: u32 = 98;
const ALL: u32 = 99;

#[derive(Clone, Debug)]
pub struct CronError(String);
impl fmt::Display for CronError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(&self.0)
    }
}
impl std::error::Error for CronError {}
fn invalid(message: impl Into<String>) -> CronError {
    CronError(message.into())
}
type Result<T> = std::result::Result<T, CronError>;

/// 解析状态属于单个表达式，可重复用于计算后续触发时间。
#[derive(Clone, Debug)]
pub struct CronExpression {
    fields: [BTreeSet<u32>; 7],
    last_day: bool,
    last_weekday: bool,
    nearest_weekday: bool,
    last_offset: u32,
    nth_weekday: u32,
    every_nth_week: u32,
}

pub fn validate(expression: &str) -> Result<()> {
    expression.parse::<CronExpression>().map(|_| ())
}

pub fn get_next_occurrence(expression: &str, after: DateTime<Utc>) -> Result<DateTime<Utc>> {
    expression
        .parse::<CronExpression>()?
        .next_after(after)?
        .ok_or_else(|| invalid("Cron 没有后续触发时间"))
}

pub fn get_next_occurrences(
    expression: &str,
    mut after: DateTime<Utc>,
    count: usize,
) -> Result<Vec<DateTime<Utc>>> {
    let expression: CronExpression = expression.parse()?;
    let mut result = Vec::new();
    for _ in 0..count {
        after = expression
            .next_after(after)?
            .ok_or_else(|| invalid("Cron 没有后续触发时间"))?;
        result.push(after);
    }
    Ok(result)
}

fn max_year() -> u32 {
    static MAX_YEAR: OnceLock<u32> = OnceLock::new();
    *MAX_YEAR.get_or_init(|| (Local::now().year() as u32 + 100).min(9999))
}

impl FromStr for CronExpression {
    type Err = CronError;
    fn from_str(expression: &str) -> Result<Self> {
        let mut result = Self {
            fields: Default::default(),
            last_day: false,
            last_weekday: false,
            nearest_weekday: false,
            last_offset: 0,
            nth_weekday: 0,
            every_nth_week: 0,
        };
        let expression = expression.to_ascii_uppercase();
        let fields: Vec<_> = expression.split_ascii_whitespace().collect();
        if fields.len() < 6 {
            return Err(invalid("Cron 至少需要秒、分、时、日、月、星期六个字段"));
        }
        // 与参考解析器一致：可选第七项为年，后续项不参与表达式。
        for (field, value) in fields.iter().take(7).enumerate() {
            if (field == DAY || field == WEEKDAY) && value.contains('L') && value.contains(',') {
                return Err(invalid("L 和 LW 不能与其他日期混用"));
            }
            if field == WEEKDAY && value.matches('#').count() > 1 {
                return Err(invalid("只能指定一个第 N 个星期"));
            }
            for item in value.split(',') {
                result.parse_item(item, field)?;
            }
        }
        if fields.len() == 6 {
            result.add(ALL, None, 0, YEAR)?;
        }
        if result.fields[DAY].contains(&UNSPECIFIED)
            == result.fields[WEEKDAY].contains(&UNSPECIFIED)
        {
            return Err(invalid("日期和星期必须且只能有一项使用 ?"));
        }
        for field in [SECOND, MINUTE, HOUR, MONTH, WEEKDAY, YEAR] {
            if result.fields[field].is_empty() {
                return Err(invalid("Cron 字段不能为空"));
            }
        }
        if result.fields[DAY].is_empty() && !result.last_day {
            return Err(invalid("日期字段不能为空"));
        }
        Ok(result)
    }
}

impl CronExpression {
    fn parse_item(&mut self, item: &str, field: usize) -> Result<()> {
        if item.is_empty() {
            return Ok(());
        }
        if item == "?" {
            if field != DAY && field != WEEKDAY {
                return Err(invalid("? 只用于日期或星期"));
            }
            return self.add(UNSPECIFIED, None, 0, field);
        }
        if item.starts_with('L') && (item == "L" || item == "LW" || item.starts_with("L-")) {
            if field == WEEKDAY && item == "L" {
                return self.add(7, None, 0, field);
            }
            if field != DAY {
                return Err(invalid("L 偏移和 LW 只用于日期"));
            }
            self.last_day = true;
            self.nearest_weekday = item.ends_with('W');
            let offset = item.strip_suffix('W').unwrap_or(item);
            if let Some(value) = offset.strip_prefix("L-") {
                self.last_offset = number(value)?;
                if self.last_offset > 30 {
                    return Err(invalid("最后一天偏移不能超过 30"));
                }
            }
            return Ok(());
        }
        if item.as_bytes()[0].is_ascii_alphabetic() {
            if field != MONTH && field != WEEKDAY {
                return Err(invalid("名称只用于月份或星期"));
            }
            let name = item.get(..3).ok_or_else(|| invalid("月份或星期名称无效"))?;
            let start = named_value(name, field)?;
            let suffix = &item[3..];
            if let Some(end) = suffix.strip_prefix('-') {
                // 名称范围按参考实现递增一；步长应使用数字范围声明。
                let end = named_value(end.get(..3).ok_or_else(|| invalid("范围终点无效"))?, field)?;
                return self.add(start, Some(end), 1, field);
            }
            if field == WEEKDAY && !suffix.is_empty() {
                if let Some(nth) = suffix.strip_prefix('#') {
                    self.nth_weekday = one_to_five(nth)?;
                } else if let Some(nth) = suffix.strip_prefix('/') {
                    self.every_nth_week = one_to_five(nth)?;
                } else if suffix == "L" {
                    self.last_weekday = true;
                } else {
                    return Err(invalid("星期名称后缀无效"));
                }
            }
            return self.add(start, None, 0, field);
        }
        if let Some(value) = item.strip_suffix('L') {
            if field != WEEKDAY {
                return Err(invalid("nL 只用于星期"));
            }
            self.last_weekday = true;
            return self.add(number(value)?, None, 0, field);
        }
        if let Some(value) = item.strip_suffix('W') {
            if field != DAY {
                return Err(invalid("W 只用于日期"));
            }
            self.nearest_weekday = true;
            return self.add(number(value)?, None, 0, field);
        }
        if let Some((value, nth)) = item.split_once('#') {
            if field != WEEKDAY {
                return Err(invalid("# 只用于星期"));
            }
            self.nth_weekday = one_to_five(nth)?;
            return self.add(number(value)?, None, 0, field);
        }
        if let Some(value) = item.strip_suffix('C') {
            if field != DAY && field != WEEKDAY {
                return Err(invalid("C 只用于日期或星期"));
            }
            // 参考实现未关联外部日历，C 的值直接进入普通日期集合。
            return self.add(number(value)?, None, 0, field);
        }
        let (range, increment) = match item.split_once('/') {
            Some((value, increment)) => (value, Some(number(increment)?)),
            None => (item, None),
        };
        let (start, end) = if range == "*" || (range.is_empty() && increment.is_some()) {
            (ALL, None)
        } else if let Some((start, end)) = range.split_once('-') {
            (number(start)?, Some(number(end)?))
        } else {
            (number(range)?, None)
        };
        let increment = increment.unwrap_or(u32::from(end.is_some()));
        if end.is_none() && field != YEAR && increment > field_bounds(field).1 {
            return Err(invalid("Cron 步长超出字段范围"));
        }
        self.add(start, end, increment, field)
    }

    fn add(&mut self, value: u32, end: Option<u32>, increment: u32, field: usize) -> Result<()> {
        let (minimum, maximum) = field_bounds(field);
        if value == UNSPECIFIED && (field == DAY || field == WEEKDAY) {
            self.fields[field].insert(value);
            return Ok(());
        }
        if value != ALL
            && (value < minimum
                || value > maximum
                || end.is_some_and(|end| end < minimum || end > maximum))
        {
            return Err(invalid("Cron 数值超出字段范围"));
        }
        if increment == 0 && value != ALL {
            self.fields[field].insert(value);
            return Ok(());
        }
        let start = if value == ALL {
            if field == YEAR { 1970 } else { minimum }
        } else {
            value
        };
        let mut end = end.unwrap_or(if field == YEAR { max_year() } else { maximum });
        let cycle = if end < start {
            if field == YEAR {
                return Err(invalid("年份范围不能倒序"));
            }
            let cycle = maximum - minimum + 1;
            end += cycle;
            Some(cycle)
        } else {
            None
        };
        for value in (start..=end).step_by(increment.max(1) as usize) {
            let value = cycle.map_or(value, |cycle| (value - minimum) % cycle + minimum);
            self.fields[field].insert(value);
        }
        Ok(())
    }

    /// 严格从给定时间后一秒开始搜索；毫秒不参与 Cron。
    /// 初始日历取宿主本地时间，最终使用本地标准偏移，与 NetEngine BaseUtcOffset 一致。
    pub fn next_after(&self, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>> {
        let next = after
            .checked_add_signed(Duration::seconds(1))
            .ok_or_else(|| invalid("时间超出范围"))?;
        let mut date = next
            .with_timezone(&Local)
            .naive_local()
            .with_nanosecond(0)
            .ok_or_else(|| invalid("时间超出范围"))?;
        loop {
            if date.year() > max_year() as i32 || date.year() > 9999 {
                return Ok(None);
            }
            let second = self.next(SECOND, date.second());
            if second.is_none() {
                date = add_duration(date, Duration::minutes(1))?;
            }
            date = date
                .with_second(second.unwrap_or_else(|| self.first(SECOND)))
                .ok_or_else(|| invalid("秒无效"))?;
            let minute = self.next(MINUTE, date.minute());
            if minute != Some(date.minute()) {
                if minute.is_none() {
                    date = add_duration(date, Duration::hours(1))?;
                }
                date = date
                    .date()
                    .and_hms_opt(date.hour(), minute.unwrap_or_else(|| self.first(MINUTE)), 0)
                    .ok_or_else(|| invalid("分钟无效"))?;
                continue;
            }
            let hour = self.next(HOUR, date.hour());
            if hour != Some(date.hour()) {
                if hour.is_none() {
                    date = add_duration(date, Duration::days(1))?;
                }
                date = date
                    .date()
                    .and_hms_opt(hour.unwrap_or_else(|| self.first(HOUR)), 0, 0)
                    .ok_or_else(|| invalid("小时无效"))?;
                continue;
            }
            let day = self.next_day(date.date())?;
            if day != date.date() {
                date = midnight(day)?;
                continue;
            }
            let month = self.next(MONTH, date.month());
            if month != Some(date.month()) {
                let year = date.year() + i32::from(month.is_none());
                date = midnight(calendar_date(
                    year,
                    month.unwrap_or_else(|| self.first(MONTH)),
                    1,
                )?)?;
                continue;
            }
            let Some(year) = self.next(YEAR, date.year() as u32) else {
                return Ok(None);
            };
            if year != date.year() as u32 {
                date = midnight(calendar_date(year as i32, 1, 1)?)?;
                continue;
            }
            return standard_offset()?
                .from_local_datetime(&date)
                .single()
                .map(|date| Some(date.with_timezone(&Utc)))
                .ok_or_else(|| invalid("触发时间超出范围"));
        }
    }

    fn first(&self, field: usize) -> u32 {
        self.fields[field].first().copied().unwrap_or_default()
    }
    fn next(&self, field: usize, value: u32) -> Option<u32> {
        self.fields[field].range(value..).next().copied()
    }

    fn next_day(&self, date: NaiveDate) -> Result<NaiveDate> {
        let last = last_day(date.year(), date.month())?;
        if !self.fields[DAY].contains(&UNSPECIFIED) {
            let target = if self.last_day {
                last.checked_sub(self.last_offset)
                    .filter(|day| *day > 0)
                    .ok_or_else(|| invalid("最后一天偏移超出当前月份"))?
            } else if self.nearest_weekday {
                self.first(DAY)
            } else {
                return match self.next(DAY, date.day()).filter(|day| *day <= last) {
                    Some(day) => calendar_date(date.year(), date.month(), day),
                    None => next_month(date),
                };
            };
            let target = if self.nearest_weekday {
                let weekday = calendar_date(date.year(), date.month(), target)?
                    .weekday()
                    .number_from_sunday();
                match weekday {
                    7 if target == 1 => target + 2,
                    7 => target - 1,
                    1 if target == last => target - 2,
                    1 => target + 1,
                    _ => target,
                }
            } else {
                target
            };
            return if target < date.day() {
                next_month(date)
            } else {
                calendar_date(date.year(), date.month(), target)
            };
        }
        let current = date.weekday().number_from_sunday();
        let weekday = self.first(WEEKDAY);
        if self.last_weekday {
            let last_date = calendar_date(date.year(), date.month(), last)?;
            let target = last - (last_date.weekday().number_from_sunday() + 7 - weekday) % 7;
            return if target < date.day() {
                next_month(date)
            } else {
                calendar_date(date.year(), date.month(), target)
            };
        }
        if self.nth_weekday != 0 {
            let first = calendar_date(date.year(), date.month(), 1)?
                .weekday()
                .number_from_sunday();
            let target = 1 + (weekday + 7 - first) % 7 + (self.nth_weekday - 1) * 7;
            return if target < date.day() || target > last {
                next_month(date)
            } else {
                calendar_date(date.year(), date.month(), target)
            };
        }
        let weekday = self.next(WEEKDAY, current).unwrap_or(weekday);
        let mut advance = (weekday + 7 - current) % 7;
        if advance != 0 && self.every_nth_week != 0 {
            advance += 7 * (self.every_nth_week - 1);
        }
        if self.every_nth_week == 0 && date.day() + advance > last {
            return next_month(date);
        }
        date.checked_add_signed(Duration::days(advance.into()))
            .ok_or_else(|| invalid("日期超出范围"))
    }
}

fn number(value: &str) -> Result<u32> {
    if value.is_empty() || !value.bytes().all(|value| value.is_ascii_digit()) {
        return Err(invalid("Cron 需要非负整数"));
    }
    value.parse().map_err(|_| invalid("Cron 数值超出范围"))
}
fn one_to_five(value: &str) -> Result<u32> {
    let value = number(value)?;
    if (1..=5).contains(&value) {
        Ok(value)
    } else {
        Err(invalid("# 或星期名称后的 / 必须指定 1 到 5"))
    }
}
fn named_value(value: &str, field: usize) -> Result<u32> {
    let names = if field == MONTH {
        &[
            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
        ][..]
    } else {
        &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"][..]
    };
    names
        .iter()
        .position(|name| *name == value)
        .map(|index| index as u32 + 1)
        .ok_or_else(|| invalid("月份或星期名称无效"))
}
fn field_bounds(field: usize) -> (u32, u32) {
    match field {
        SECOND | MINUTE => (0, 59),
        HOUR => (0, 23),
        DAY => (1, 31),
        MONTH => (1, 12),
        WEEKDAY => (1, 7),
        _ => (0, 9999),
    }
}
fn calendar_date(year: i32, month: u32, day: u32) -> Result<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| invalid("Cron 日期超出当前日历范围"))
}
fn midnight(date: NaiveDate) -> Result<NaiveDateTime> {
    date.and_hms_opt(0, 0, 0)
        .ok_or_else(|| invalid("日期超出范围"))
}
fn next_month(date: NaiveDate) -> Result<NaiveDate> {
    if date.month() == 12 {
        calendar_date(date.year() + 1, 1, 1)
    } else {
        calendar_date(date.year(), date.month() + 1, 1)
    }
}
fn last_day(year: i32, month: u32) -> Result<u32> {
    let first = calendar_date(year, month, 1)?;
    Ok(next_month(first)?
        .pred_opt()
        .ok_or_else(|| invalid("日期超出范围"))?
        .day())
}
fn add_duration(date: NaiveDateTime, duration: Duration) -> Result<NaiveDateTime> {
    date.checked_add_signed(duration)
        .ok_or_else(|| invalid("日期超出范围"))
}

fn standard_offset() -> Result<FixedOffset> {
    static OFFSET: OnceLock<std::result::Result<FixedOffset, CronError>> = OnceLock::new();
    OFFSET.get_or_init(local_standard_offset).clone()
}

#[cfg(unix)]
fn local_standard_offset() -> Result<FixedOffset> {
    crate::date_time_helper::standard_offset_at(Local::now().naive_local())
        .ok_or_else(|| invalid("无法取得本地标准时区偏移"))
}

#[cfg(windows)]
fn local_standard_offset() -> Result<FixedOffset> {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        weekday: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[repr(C)]
    #[derive(Default)]
    struct TimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SystemTime,
        daylight_bias: i32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetTimeZoneInformation(information: *mut TimeZoneInformation) -> u32;
    }
    let mut information = TimeZoneInformation::default();
    // SAFETY: 结构布局与 Win32 TIME_ZONE_INFORMATION 一致，输出存储有效。
    if unsafe { GetTimeZoneInformation(&mut information) } == u32::MAX {
        return Err(invalid("读取 Windows 时区失败"));
    }
    information
        .bias
        .checked_add(information.standard_bias)
        .and_then(|bias| bias.checked_mul(-60))
        .and_then(FixedOffset::east_opt)
        .ok_or_else(|| invalid("本地时区偏移超出范围"))
}

#[cfg(not(any(unix, windows)))]
fn local_standard_offset() -> Result<FixedOffset> {
    Err(invalid("当前平台不支持本地标准时区查询"))
}
