use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use event_bus::{EventEnvelope, EventRecord, EventResult, EventTransport};
use redis::{aio::ConnectionManager, streams::StreamRangeReply};
use std::collections::HashMap;
use tokio::sync::OnceCell;

/// Redis Stream 独立游标广播，不使用竞争消费组或删除流重置位置。
pub struct RedisEventTransport {
    client: redis::Client,
    manager: OnceCell<ConnectionManager>,
    key: String,
}
impl RedisEventTransport {
    pub fn new(connection: &str, scope: &str) -> EventResult<Self> {
        Ok(Self {
            client: redis::Client::open(connection).map_err(|error| error.to_string())?,
            manager: OnceCell::new(),
            key: format!("eventbus:{scope}"),
        })
    }
    async fn connection(&self) -> EventResult<ConnectionManager> {
        self.manager
            .get_or_try_init(|| async { ConnectionManager::new(self.client.clone()).await })
            .await
            .cloned()
            .map_err(|error| error.to_string())
    }
}
#[async_trait]
impl EventTransport for RedisEventTransport {
    async fn position(&self) -> EventResult<String> {
        let mut connection = self.connection().await?;
        let reply: StreamRangeReply = redis::cmd("XREVRANGE")
            .arg(&self.key)
            .arg("+")
            .arg("-")
            .arg("COUNT")
            .arg(1)
            .query_async(&mut connection)
            .await
            .map_err(|error| error.to_string())?;
        Ok(reply
            .ids
            .into_iter()
            .next()
            .map(|entry| entry.id)
            .unwrap_or_else(|| "0-0".into()))
    }
    async fn append(&self, message: EventEnvelope) -> EventResult<()> {
        let mut connection = self.connection().await?;
        redis::cmd("XADD")
            .arg(&self.key)
            .arg("*")
            .arg("event-id")
            .arg(message.event_id.simple().to_string())
            .arg("event-name")
            .arg(message.event_name)
            .arg("payload")
            .arg(message.payload)
            .query_async::<String>(&mut connection)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    async fn read(&self, position: &str, count: usize) -> EventResult<Vec<EventRecord>> {
        if !(1..=1000).contains(&count) {
            return Err("事件读取数量超出范围".into());
        }
        let mut connection = self.connection().await?;
        // StreamRangeReply 的 HashMap 会覆盖重复字段，无法区分无效记录。
        let entries: Vec<(String, Vec<redis::Value>)> = redis::cmd("XRANGE")
            .arg(&self.key)
            .arg(format!("({position}"))
            .arg("+")
            .arg("COUNT")
            .arg(count)
            .query_async(&mut connection)
            .await
            .map_err(|error| error.to_string())?;
        Ok(entries.into_iter().map(|(position, fields)| {
            let parsed = parse(&position, &fields);
            match parsed {
                Ok((published_at, message)) => EventRecord { position, published_at, message: Some(message) },
                Err(error) => {
                    tracing::warn!(target: "event_bus.redis", %error, %position, "跳过无效事件记录");
                    EventRecord { position, published_at: DateTime::<Utc>::MIN_UTC, message: None }
                }
            }
        }).collect())
    }
    async fn trim(&self, before: DateTime<Utc>) -> EventResult<()> {
        let mut connection = self.connection().await?;
        redis::cmd("XTRIM")
            .arg(&self.key)
            .arg("MINID")
            .arg("=")
            .arg(format!("{}-0", before.timestamp_millis().max(0)))
            .query_async::<u64>(&mut connection)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    async fn health(&self) -> EventResult<()> {
        let mut connection = self.connection().await?;
        redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
fn parse(id: &str, fields: &[redis::Value]) -> EventResult<(DateTime<Utc>, EventEnvelope)> {
    let (pairs, remainder) = fields.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err("事件字段必须成对出现".into());
    }
    let mut values = HashMap::with_capacity(pairs.len());
    for [name, value] in pairs {
        let name = field_text(name)?;
        let value = field_text(value)?;
        if values.insert(name, value).is_some() {
            return Err("事件记录包含重复字段".into());
        }
    }
    let milliseconds = id
        .split_once('-')
        .ok_or("事件位置格式无效")?
        .0
        .parse::<i64>()
        .map_err(|error| error.to_string())?;
    let published_at = DateTime::from_timestamp_millis(milliseconds)
        .filter(|time| (1..=9999).contains(&time.year()))
        .ok_or("事件时间超出范围")?;
    let field = |name: &str| -> EventResult<String> {
        values
            .get(name)
            .cloned()
            .ok_or_else(|| format!("缺少事件字段 {name}"))
    };
    let event_name = field("event-name")?;
    event_bus::validate_name(&event_name)?;
    Ok((
        published_at,
        EventEnvelope {
            event_id: common::data_helper::parse_guid(&field("event-id")?)
                .ok_or("事件标识格式无效")?,
            event_name,
            payload: field("payload")?,
        },
    ))
}

/// Redis 的字符串视图与源客户端一致，以替换字符处理非法 UTF-8。
fn field_text(value: &redis::Value) -> EventResult<String> {
    match value {
        redis::Value::BulkString(bytes) => Ok(String::from_utf8_lossy(bytes).into_owned()),
        value => redis::from_redis_value(value.clone()).map_err(|error| error.to_string()),
    }
}
