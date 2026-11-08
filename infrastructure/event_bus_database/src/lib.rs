pub mod schema;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use event_bus::{CancellationToken, EventEnvelope, EventRecord, EventResult, EventTransport};
use identifier_generator::IdentifierGenerator;
use repository_core::postgres_operation::PostgresOperation;
use sea_orm::sqlx::{self, Row};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use tokio::sync::OnceCell;
use uuid::Uuid;

const MINIMUM_TIME: DateTime<Utc> =
    DateTime::from_timestamp(-62_135_596_800, 0).expect("0001-01-01 UTC");
const MAXIMUM_TIME: DateTime<Utc> =
    DateTime::from_timestamp(253_402_300_799, 999_999_900).expect("DateTimeOffset.MaxValue UTC");

/// 独立写连接与发布事务；按 Scope 的状态行写锁保证位置顺序等于提交顺序。
pub struct DatabaseEventTransport {
    options: ConnectOptions,
    db: OnceCell<DatabaseConnection>,
    scope: String,
    ids: IdentifierGenerator,
}
impl DatabaseEventTransport {
    pub fn new(options: ConnectOptions, scope: &str, ids: IdentifierGenerator) -> Self {
        Self {
            options,
            db: OnceCell::new(),
            scope: scope.into(),
            ids,
        }
    }
    async fn db(&self) -> EventResult<&DatabaseConnection> {
        self.db
            .get_or_try_init(|| Database::connect(self.options.clone()))
            .await
            .map_err(|error| error.to_string())
    }
    async fn operation(&self, stopping: &CancellationToken) -> EventResult<PostgresOperation> {
        let db = tokio::select! {
            biased;
            _ = stopping.cancelled() => return Err("事件数据库操作已取消".into()),
            result = self.db() => result?,
        };
        PostgresOperation::acquire(db.get_postgres_connection_pool(), stopping).await
    }
    async fn ensure_stream(&self, stopping: &CancellationToken) -> EventResult<i64> {
        let mut operation = self.operation(stopping).await?;
        let scope = self.scope.clone();
        let position = operation
            .query(move |connection| {
                Box::pin(async move {
                    sqlx::query_scalar::<_, i64>(
                        r#"SELECT "Position" FROM "EventBusStream" WHERE "Scope" = $1"#,
                    )
                    .bind(scope)
                    .fetch_optional(connection)
                    .await
                })
            })
            .await?;
        let position = if let Some(position) = position {
            position
        } else {
            let id = tokio::select! {
                biased;
                _ = stopping.cancelled() => return Err(operation.cancel().await),
                result = self.ids.get_id() => result.map_err(|error| error.to_string())?,
            };
            let scope = self.scope.clone();
            operation.query(move |connection| Box::pin(async move {
                sqlx::query(r#"INSERT INTO "EventBusStream" ("Id", "Scope", "Position", "CreateTime") VALUES ($1, $2, 0, $3) ON CONFLICT ("Scope") DO NOTHING"#)
                    .bind(id).bind(scope).bind(Utc::now()).execute(connection).await
            })).await?;
            let scope = self.scope.clone();
            operation
                .query(move |connection| {
                    Box::pin(async move {
                        sqlx::query_scalar::<_, i64>(
                            r#"SELECT "Position" FROM "EventBusStream" WHERE "Scope" = $1"#,
                        )
                        .bind(scope)
                        .fetch_one(connection)
                        .await
                    })
                })
                .await?
        };
        operation.commit().await?;
        Ok(position)
    }
}
struct MessageRow {
    position: i64,
    event_id: Uuid,
    event_name: String,
    payload: String,
    create_time: Option<DateTime<Utc>>,
    time_infinity: i32,
}
impl MessageRow {
    fn from_row(row: sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            position: row.try_get("Position")?,
            event_id: row.try_get("EventId")?,
            event_name: row.try_get("EventName")?,
            payload: row.try_get("Payload")?,
            create_time: row.try_get("CreateTime")?,
            time_infinity: row.try_get("TimeInfinity")?,
        })
    }
    fn into_record(self) -> EventResult<EventRecord> {
        let published_at = match self.time_infinity {
            -1 => MINIMUM_TIME,
            1 => MAXIMUM_TIME,
            _ => self.create_time.ok_or("事件创建时间超出支持范围")?,
        };
        Ok(EventRecord {
            position: self.position.to_string(),
            published_at,
            message: Some(EventEnvelope {
                event_id: self.event_id,
                event_name: self.event_name,
                payload: self.payload,
            }),
        })
    }
}
#[async_trait]
impl EventTransport for DatabaseEventTransport {
    async fn position(&self) -> EventResult<String> {
        self.position_with_cancellation(&CancellationToken::new())
            .await
    }
    async fn position_with_cancellation(
        &self,
        stopping: &CancellationToken,
    ) -> EventResult<String> {
        self.ensure_stream(stopping)
            .await
            .map(|position| position.to_string())
    }
    async fn append(&self, message: EventEnvelope) -> EventResult<()> {
        self.append_with_cancellation(message, &CancellationToken::new())
            .await
    }
    async fn append_with_cancellation(
        &self,
        message: EventEnvelope,
        stopping: &CancellationToken,
    ) -> EventResult<()> {
        // 范围初始化独立提交；广播的状态递增和信封写入共享下一笔事务。
        self.ensure_stream(stopping).await?;
        let mut operation = self.operation(stopping).await?;
        let scope = self.scope.clone();
        let position = operation.query(move |connection| Box::pin(async move {
            sqlx::query_scalar::<_, i64>(
                r#"UPDATE "EventBusStream" SET "Position" = "Position" + 1 WHERE "Scope" = $1 RETURNING "Position""#,
            ).bind(scope).fetch_one(connection).await
        })).await?;
        let id = tokio::select! {
            biased;
            _ = stopping.cancelled() => return Err(operation.cancel().await),
            result = self.ids.get_id() => result.map_err(|error| error.to_string())?,
        };
        let scope = self.scope.clone();
        operation.query(move |connection| Box::pin(async move {
            sqlx::query(
                r#"INSERT INTO "EventBusMessage" ("Id", "Scope", "Position", "EventId", "EventName", "Payload", "CreateTime") VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            ).bind(id).bind(scope).bind(position).bind(message.event_id)
                .bind(message.event_name).bind(message.payload).bind(Utc::now())
                .execute(connection).await
        })).await?;
        operation.commit().await
    }
    async fn read(&self, position: &str, count: usize) -> EventResult<Vec<EventRecord>> {
        self.read_with_cancellation(position, count, &CancellationToken::new())
            .await
    }
    async fn read_with_cancellation(
        &self,
        position: &str,
        count: usize,
        stopping: &CancellationToken,
    ) -> EventResult<Vec<EventRecord>> {
        if !(1..=1000).contains(&count) {
            return Err("事件读取数量超出范围".into());
        }
        let position = position.parse::<i64>().map_err(|error| error.to_string())?;
        // 先在 SQL 侧隔离无限时间与 .NET 日期范围外的值，避免驱动将其直接解码为
        // Chrono 日期时溢出。无限值对应 Npgsql 的 MinValue/MaxValue，有限越界值报错。
        let mut operation = self.operation(stopping).await?;
        let scope = self.scope.clone();
        let rows = operation
            .query(move |connection| {
                Box::pin(async move {
                    sqlx::query(
                        r#"SELECT "Position", "EventId", "EventName", "Payload",
                CASE WHEN "CreateTime" BETWEEN '0001-01-01 00:00:00+00'::timestamptz
                    AND '9999-12-31 23:59:59.999999+00'::timestamptz
                    THEN "CreateTime" ELSE NULL END AS "CreateTime",
                CASE WHEN "CreateTime" = '-infinity'::timestamptz THEN -1
                    WHEN "CreateTime" = 'infinity'::timestamptz THEN 1
                    ELSE 0 END AS "TimeInfinity"
                FROM "EventBusMessage" WHERE "Scope" = $1 AND "Position" > $2
                    AND "DeleteTime" IS NULL ORDER BY "Position" LIMIT $3"#,
                    )
                    .bind(scope)
                    .bind(position)
                    .bind(count as i64)
                    .fetch_all(connection)
                    .await
                })
            })
            .await?;
        operation.commit().await?;
        rows.into_iter()
            .map(|row| {
                MessageRow::from_row(row)
                    .map_err(|error| error.to_string())?
                    .into_record()
            })
            .collect()
    }
    async fn trim(&self, before: DateTime<Utc>) -> EventResult<()> {
        self.trim_with_cancellation(before, &CancellationToken::new())
            .await
    }
    async fn trim_with_cancellation(
        &self,
        before: DateTime<Utc>,
        stopping: &CancellationToken,
    ) -> EventResult<()> {
        // Npgsql 默认将 DateTimeOffset.MinValue 写为 -infinity；不能用有限的公元 1 年
        // 替代，否则极长保留时间会误删 -infinity 和更早年份的已有消息。
        let mut operation = self.operation(stopping).await?;
        let scope = self.scope.clone();
        operation.query(move |connection| Box::pin(async move {
            if before == MINIMUM_TIME {
                sqlx::query(
                r#"DELETE FROM "EventBusMessage" WHERE "Scope" = $1 AND "CreateTime" < '-infinity'::timestamptz"#,
                ).bind(scope).execute(connection).await
        } else {
                sqlx::query(
                r#"DELETE FROM "EventBusMessage" WHERE "Scope" = $1 AND "CreateTime" < $2"#,
                ).bind(scope).bind(before).execute(connection).await
            }
        })).await?;
        operation.commit().await
    }
    async fn health(&self) -> EventResult<()> {
        self.health_with_cancellation(&CancellationToken::new())
            .await
    }
    async fn health_with_cancellation(&self, stopping: &CancellationToken) -> EventResult<()> {
        let db = tokio::select! {
            biased;
            _ = stopping.cancelled() => return Err("事件数据库操作已取消".into()),
            result = self.db() => result?,
        };
        repository_core::database_health::check_database_health_with_cancellation(db, stopping)
            .await
            .map_err(|error| error.to_string())
    }
}
