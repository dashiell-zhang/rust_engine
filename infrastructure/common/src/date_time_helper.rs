use chrono::{
    DateTime, Datelike, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, Offset, TimeZone,
    Utc,
};
use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::{Duration as StdDuration, SystemTime},
};
use uuid::Uuid;

mod local;
pub(crate) use local::resolve_local;
#[cfg(unix)]
#[cfg(feature = "runtime")]
pub(crate) use local::standard_offset_at;

/// 转换为 TimeSpan 的有符号 100ns 计数；不足一个 tick 的部分向零截断。
/// TimeDelta 的可表示范围更大，越过 TimeSpan 的 i64 范围时返回 None。
pub fn time_span_ticks(value: Duration) -> Option<i64> {
    let ticks =
        i128::from(value.num_seconds()) * 10_000_000 + i128::from(value.subsec_nanos()) / 100;
    i64::try_from(ticks).ok()
}

/// 将有符号 TimeSpan ticks 转换为可直接参与日期运算的时间差。
pub fn time_span_from_ticks(ticks: i64) -> Duration {
    Duration::seconds(ticks / 10_000_000) + Duration::nanoseconds(ticks % 10_000_000 * 100)
}

/// 获取某个日期所属周第一天
pub fn get_week_first_day(date: NaiveDate) -> NaiveDate {
    let weekday = date.weekday().num_days_from_monday() as i64;
    date - Duration::days(weekday)
}

/// 获取某个日期所属季度
pub fn get_quarter(date: NaiveDate) -> u32 {
    ((date.month() - 1) / 3) + 1
}

/// 获取某个日期所属季度
pub fn get_quarterly(date: NaiveDate) -> u32 {
    get_quarter(date)
}

/// 获取某个日期所属季度第一天
pub fn get_quarter_first_day(date: NaiveDate) -> NaiveDate {
    let first_month = match get_quarter(date) {
        1 => 1,
        2 => 4,
        3 => 7,
        4 => 10,
        _ => unreachable!(),
    };

    NaiveDate::from_ymd_opt(date.year(), first_month, 1).expect("季度第一天应始终有效")
}

/// 获取某个日期所属季度第一天
pub fn get_quarterly_first_day(date: NaiveDate) -> NaiveDate {
    get_quarter_first_day(date)
}

/// 获取 NTP 网络远程时间
pub fn get_network_time() -> std::io::Result<DateTime<Utc>> {
    let server = ("ntp.tencent.com", 123)
        .to_socket_addrs()?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| std::io::Error::other("未获取到可用的NTP IPv4地址"))?;
    get_network_time_from(server)
}

/// 从指定 NTP 地址获取时间
fn get_network_time_from(server: SocketAddr) -> std::io::Result<DateTime<Utc>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(StdDuration::from_secs(3)))?;
    socket.connect(server)?;

    let mut packet = [0u8; 48];
    packet[0] = 0x1B;
    socket.send(&packet)?;
    let received = socket.recv(&mut packet)?;
    if received < packet.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "NTP响应长度不足48字节",
        ));
    }

    let leap_indicator = packet[0] >> 6;
    let mode = packet[0] & 0x07;
    let stratum = packet[1];
    if leap_indicator == 3 || mode != 4 || !(1..=15).contains(&stratum) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "NTP响应状态无效",
        ));
    }

    let seconds_part = u32::from_be_bytes([packet[40], packet[41], packet[42], packet[43]]);
    let fraction = u32::from_be_bytes([packet[44], packet[45], packet[46], packet[47]]) as u64;
    if seconds_part == 0 && fraction == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "NTP响应未包含服务器发送时间",
        ));
    }

    const NTP_UNIX_OFFSET_SECONDS: i64 = 2_208_988_800;
    const NTP_ERA_SECONDS: u64 = 1_u64 << 32;

    let current_ntp_seconds = Utc::now()
        .timestamp()
        .checked_add(NTP_UNIX_OFFSET_SECONDS)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| std::io::Error::other("当前时间早于 NTP 纪元"))?;
    let current_era = current_ntp_seconds / NTP_ERA_SECONDS;
    let mut seconds = u64::from(seconds_part) + current_era * NTP_ERA_SECONDS;

    if seconds > current_ntp_seconds + NTP_ERA_SECONDS / 2 && seconds >= NTP_ERA_SECONDS {
        seconds -= NTP_ERA_SECONDS;
    } else if seconds + NTP_ERA_SECONDS / 2 < current_ntp_seconds {
        seconds = seconds
            .checked_add(NTP_ERA_SECONDS)
            .ok_or_else(|| std::io::Error::other("NTP 时代换算溢出"))?;
    }

    // 源助手按整数毫秒截断小数部分，不将 NTP 小数扩展为纳秒。
    let unix_milliseconds = seconds
        .checked_mul(1000)
        .and_then(|value| value.checked_add(fraction * 1000 / NTP_ERA_SECONDS))
        .and_then(|value| i64::try_from(value).ok())
        .and_then(|value| value.checked_sub(NTP_UNIX_OFFSET_SECONDS * 1000))
        .ok_or_else(|| std::io::Error::other("NTP 时间超出支持范围"))?;

    datetime_from_unix_milliseconds(unix_milliseconds)
        .ok_or_else(|| std::io::Error::other("NTP 时间转换失败"))
}

/// 通过时间格式字符串获取时间
pub fn get_time_by_string(
    time_text: &str,
    format: &str,
) -> Result<DateTime<Utc>, chrono::ParseError> {
    chrono::NaiveDateTime::parse_from_str(time_text, format)
        .map(|value| Utc.from_utc_datetime(&value))
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(time_text, format).map(|value| {
                let date_time = value
                    .and_hms_opt(0, 0, 0)
                    .expect("日期转换为午夜时间应始终有效");
                Utc.from_utc_datetime(&date_time)
            })
        })
}

/// 年龄计算
pub fn get_age(date: NaiveDate) -> i32 {
    get_age_with_today(date, Local::now().date_naive())
}

/// 按指定当前日期计算年龄
pub fn get_age_with_today(date: NaiveDate, today: NaiveDate) -> i32 {
    let mut age = today.year() - date.year();
    // DateOnly.AddYears 在目标年非闰年时将 2 月 29 日落到 2 月 28 日。
    let birthday = date
        .with_year(today.year())
        .or_else(|| date.with_day(28)?.with_year(today.year()))
        .expect("同一年内的生日应有效");
    if today < birthday {
        age -= 1;
    }

    age.max(0)
}

/// 时间抹零
pub fn time_erase(date_time: NaiveDateTime) -> NaiveDateTime {
    date_time
        .date()
        .and_hms_opt(0, 0, 0)
        .expect("抹零后的时间应始终有效")
}

/// 带时区时间抹零并保留偏移量
pub fn time_erase_with_timezone<Tz>(date_time: DateTime<Tz>) -> Option<DateTime<FixedOffset>>
where
    Tz: TimeZone,
{
    let offset = date_time.offset().fix();
    let seconds = offset.local_minus_utc();
    if seconds % 60 != 0
        || seconds.abs() > 14 * 3600
        || !(1..=9999).contains(&date_time.naive_utc().year())
    {
        return None;
    }
    let local = date_time.naive_utc().checked_add_offset(offset)?;
    if !(1..=9999).contains(&local.year()) {
        return None;
    }
    let naive = local.date().and_hms_opt(0, 0, 0)?;
    let result = offset.from_local_datetime(&naive).single()?;
    (1..=9999)
        .contains(&result.naive_utc().year())
        .then_some(result)
}

/// 获取 UUIDv7 中的时间信息
pub fn get_time_from_uuid_v7(uuid: Uuid) -> Option<SystemTime> {
    let date_time = get_time_from_guid_v7(uuid).ok()?;
    SystemTime::UNIX_EPOCH
        .checked_add(StdDuration::from_millis(date_time.timestamp_millis() as u64))
}

/// 按源 Guid 文本的版本位检查 V7，并返回零偏移的毫秒时间；不额外限制 variant。
pub fn get_time_from_guid_v7(uuid: Uuid) -> Result<DateTime<Utc>, String> {
    let bytes = uuid.as_bytes();
    if bytes[6] >> 4 != 7 {
        return Err("输入Guid不是Guid V7".to_owned());
    }
    let milliseconds = bytes[..6]
        .iter()
        .fold(0i64, |value, byte| (value << 8) | i64::from(*byte));
    datetime_from_unix_milliseconds(milliseconds)
        .ok_or_else(|| "Guid V7 时间超出 DateTimeOffset 范围".to_owned())
}

fn datetime_from_unix_milliseconds(milliseconds: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(milliseconds)
        .filter(|date_time| (1..=9999).contains(&date_time.year()))
}
