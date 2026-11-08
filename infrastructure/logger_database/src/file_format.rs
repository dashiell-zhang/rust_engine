use chrono::{DateTime, Utc};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
};

/// 文件协议沿用 NetEngine 的驼峰字段、字符串 long 和带偏移的秒级时间。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileLine<'a> {
    #[serde(serialize_with = "serialize_id")]
    pub id: i64,
    pub project: &'a str,
    pub machine_name: &'a str,
    pub level: &'a str,
    pub category: &'a str,
    pub content: &'a str,
    #[serde(serialize_with = "serialize_time")]
    pub create_time: DateTime<Utc>,
    pub delete_time: Option<DateTime<Utc>>,
    pub row_version: u32,
}

fn serialize_id<S: Serializer>(id: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(id)
}

fn serialize_time<S: Serializer>(time: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&time.format("%Y/%m/%d %H:%M:%S %:z"))
}

pub(crate) struct DatabaseFileLogLine {
    pub id: i64,
    pub project: String,
    pub machine_name: String,
    pub level: String,
    pub category: String,
    pub content: String,
    pub create_time: DateTime<Utc>,
    pub delete_time: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct LogId(
    #[serde(deserialize_with = "common::json_helper::deserialize_i64_from_string_or_number")] i64,
);

struct LogTime(DateTime<Utc>);
impl<'de> Deserialize<'de> for LogTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        DateTime::parse_from_rfc3339(&value)
            .or_else(|_| DateTime::parse_from_str(&value, "%Y/%m/%d %H:%M:%S %:z"))
            .map(|value| Self(value.with_timezone(&Utc)))
            .map_err(de::Error::custom)
    }
}

/// 字段名不区分大小写，且不为已知字段额外分配字符串。
enum Field {
    Id,
    Project,
    MachineName,
    Level,
    Category,
    Content,
    CreateTime,
    DeleteTime,
    RowVersion,
    Unknown,
}
impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FieldVisitor;
        impl Visitor<'_> for FieldVisitor {
            type Value = Field;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("日志字段名")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Field, E> {
                Ok(if value.eq_ignore_ascii_case("id") {
                    Field::Id
                } else if value.eq_ignore_ascii_case("project") {
                    Field::Project
                } else if value.eq_ignore_ascii_case("machineName") {
                    Field::MachineName
                } else if value.eq_ignore_ascii_case("level") {
                    Field::Level
                } else if value.eq_ignore_ascii_case("category") {
                    Field::Category
                } else if value.eq_ignore_ascii_case("content") {
                    Field::Content
                } else if value.eq_ignore_ascii_case("createTime") {
                    Field::CreateTime
                } else if value.eq_ignore_ascii_case("deleteTime") {
                    Field::DeleteTime
                } else if value.eq_ignore_ascii_case("rowVersion") {
                    Field::RowVersion
                } else {
                    Field::Unknown
                })
            }
        }
        deserializer.deserialize_identifier(FieldVisitor)
    }
}

impl<'de> Deserialize<'de> for DatabaseFileLogLine {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LogVisitor;
        impl<'de> Visitor<'de> for LogVisitor {
            type Value = DatabaseFileLogLine;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("日志对象")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let (mut id, mut project, mut machine_name, mut level, mut category, mut content) =
                    (0, None, None, None, None, None);
                let mut create_time = Utc::now();
                let mut delete_time = None;
                while let Some(field) = map.next_key::<Field>()? {
                    match field {
                        Field::Id => id = map.next_value::<LogId>()?.0,
                        Field::Project => project = map.next_value()?,
                        Field::MachineName => machine_name = map.next_value()?,
                        Field::Level => level = map.next_value()?,
                        Field::Category => category = map.next_value()?,
                        Field::Content => content = map.next_value()?,
                        Field::CreateTime => create_time = map.next_value::<LogTime>()?.0,
                        Field::DeleteTime => {
                            delete_time = map.next_value::<Option<LogTime>>()?.map(|value| value.0)
                        }
                        Field::RowVersion => {
                            let _ = map.next_value::<u32>()?;
                        }
                        Field::Unknown => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(DatabaseFileLogLine {
                    id,
                    project: project.ok_or_else(|| de::Error::missing_field("project"))?,
                    machine_name: machine_name
                        .ok_or_else(|| de::Error::missing_field("machineName"))?,
                    level: level.ok_or_else(|| de::Error::missing_field("level"))?,
                    category: category.ok_or_else(|| de::Error::missing_field("category"))?,
                    content: content.ok_or_else(|| de::Error::missing_field("content"))?,
                    create_time,
                    delete_time,
                })
            }
        }
        deserializer.deserialize_map(LogVisitor)
    }
}
