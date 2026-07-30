use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::{Duration as StdDuration, SystemTime},
};
use uuid::Uuid;

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
    let servers = ("ntp.tencent.com", 123)
        .to_socket_addrs()?
        .collect::<Vec<_>>();
    if servers.is_empty() {
        return Err(std::io::Error::other("NTP 地址解析失败"));
    }

    let mut last_error = None;
    for server in servers {
        match get_network_time_from(server) {
            Ok(value) => return Ok(value),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| std::io::Error::other("NTP 时间获取失败")))
}

/// 从指定 NTP 地址获取时间
fn get_network_time_from(server: SocketAddr) -> std::io::Result<DateTime<Utc>> {
    let bind_addr = if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };

    let socket = UdpSocket::bind(bind_addr)?;
    socket.set_read_timeout(Some(StdDuration::from_secs(3)))?;
    socket.connect(server)?;

    let mut packet = [0u8; 48];
    packet[0] = 0x1B;
    socket.send(&packet)?;
    let received = socket.recv(&mut packet)?;
    if received < packet.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "NTP 响应长度不足",
        ));
    }

    let leap_indicator = packet[0] >> 6;
    let mode = packet[0] & 0x07;
    let stratum = packet[1];
    if leap_indicator == 3 || mode != 4 || !(1..=15).contains(&stratum) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "NTP 响应状态无效",
        ));
    }

    let seconds_part = u32::from_be_bytes([packet[40], packet[41], packet[42], packet[43]]);
    let fraction = u32::from_be_bytes([packet[44], packet[45], packet[46], packet[47]]) as u64;
    if seconds_part == 0 && fraction == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "NTP 响应未包含服务器发送时间",
        ));
    }

    const NTP_UNIX_OFFSET_SECONDS: u64 = 2_208_988_800;
    const NTP_ERA_SECONDS: u64 = 1_u64 << 32;

    let current_unix_seconds = u64::try_from(Utc::now().timestamp())
        .map_err(|_| std::io::Error::other("当前时间早于 Unix 纪元"))?;
    let current_ntp_seconds = current_unix_seconds + NTP_UNIX_OFFSET_SECONDS;
    let current_era = current_ntp_seconds / NTP_ERA_SECONDS;
    let mut seconds = u64::from(seconds_part) + current_era * NTP_ERA_SECONDS;

    if seconds > current_ntp_seconds + NTP_ERA_SECONDS / 2 && seconds >= NTP_ERA_SECONDS {
        seconds -= NTP_ERA_SECONDS;
    } else if seconds + NTP_ERA_SECONDS / 2 < current_ntp_seconds {
        seconds = seconds
            .checked_add(NTP_ERA_SECONDS)
            .ok_or_else(|| std::io::Error::other("NTP 时代换算溢出"))?;
    }

    let unix_seconds = seconds
        .checked_sub(NTP_UNIX_OFFSET_SECONDS)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| std::io::Error::other("NTP 时间超出支持范围"))?;
    let nanos = ((fraction * 1_000_000_000) >> 32) as u32;

    DateTime::<Utc>::from_timestamp(unix_seconds, nanos)
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
    get_age_with_today(date, Utc::now().date_naive())
}

/// 按指定当前日期计算年龄
pub fn get_age_with_today(date: NaiveDate, today: NaiveDate) -> i32 {
    let mut age = today.year() - date.year();
    if (today.month(), today.day()) < (date.month(), date.day()) {
        age -= 1;
    }

    age
}

/// 时间抹零
pub fn time_erase(date_time: NaiveDateTime) -> NaiveDateTime {
    date_time
        .date()
        .and_hms_opt(0, 0, 0)
        .expect("抹零后的时间应始终有效")
}

/// 带时区时间抹零并保留偏移量
pub fn time_erase_with_timezone<Tz>(date_time: DateTime<Tz>) -> Option<DateTime<Tz>>
where
    Tz: TimeZone,
{
    let naive = date_time
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("抹零后的时间应始终有效");

    date_time
        .timezone()
        .from_local_datetime(&naive)
        .single()
        .or_else(|| date_time.timezone().from_local_datetime(&naive).earliest())
}

/// 获取 UUIDv7 中的时间信息
pub fn get_time_from_uuid_v7(uuid: Uuid) -> Option<SystemTime> {
    uuid.get_timestamp().map(|timestamp| {
        let (seconds, nanos) = timestamp.to_unix();
        SystemTime::UNIX_EPOCH + std::time::Duration::new(seconds, nanos)
    })
}
