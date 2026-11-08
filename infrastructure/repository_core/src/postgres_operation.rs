use cancellation::CancellationToken;
use sea_orm::sqlx::{self, Connection, PgConnection, PgPool, Postgres, Row, pool::PoolConnection};
use std::{future::Future, pin::Pin};
use uuid::Uuid;

const CANCELLED: &str = "数据库操作已取消";

/// 每个操作独占一个事务连接，取消时不允许该连接重新进入连接池。
pub struct PostgresOperation {
    connection: PoolConnection<Postgres>,
    pool: PgPool,
    stopping: CancellationToken,
    identity: Option<(i32, chrono::DateTime<chrono::Utc>, String)>,
    finished: bool,
}

impl PostgresOperation {
    pub async fn acquire(pool: &PgPool, stopping: &CancellationToken) -> Result<Self, String> {
        let connection = tokio::select! {
            biased;
            _ = stopping.cancelled() => return Err(CANCELLED.into()),
            result = pool.acquire() => result.map_err(|error| error.to_string())?,
        };
        let mut operation = Self {
            connection,
            pool: pool.clone(),
            stopping: stopping.clone(),
            identity: None,
            finished: false,
        };
        operation
            .query(|connection| {
                Box::pin(async move { sqlx::query("BEGIN").execute(connection).await })
            })
            .await?;
        let marker = format!("database-operation:{}", Uuid::new_v4());
        let name = marker.clone();
        let row = operation.query(move |connection| Box::pin(async move {
            sqlx::query("SELECT pid, backend_start, set_config('application_name', $1, true) FROM pg_stat_activity WHERE pid = pg_backend_pid()")
                .bind(name).fetch_one(connection).await
        })).await?;
        operation.identity = Some((
            row.try_get("pid").map_err(|error| error.to_string())?,
            row.try_get("backend_start")
                .map_err(|error| error.to_string())?,
            marker,
        ));
        Ok(operation)
    }

    pub async fn query<T, F>(&mut self, action: F) -> Result<T, String>
    where
        T: Send,
        F: for<'a> FnOnce(
            &'a mut PgConnection,
        )
            -> Pin<Box<dyn Future<Output = Result<T, sqlx::Error>> + Send + 'a>>,
    {
        if self.stopping.is_cancelled() {
            return Err(self.cancel().await);
        }
        let result = {
            let query = action(&mut self.connection);
            tokio::pin!(query);
            tokio::select! {
                biased;
                result = &mut query => Some(result),
                _ = self.stopping.cancelled() => None,
            }
        };
        match result {
            Some(result) => result.map_err(|error| error.to_string()),
            None => Err(self.cancel().await),
        }
    }

    pub async fn cancel(&mut self) -> String {
        self.connection.close_on_drop();
        let cancelled =
            tokio::time::timeout(std::time::Duration::from_secs(10), self.cancel_backend()).await;
        if !matches!(cancelled, Ok(Ok(()))) {
            // 停机路径不回写日志链路，也不输出连接信息。
            eprintln!("数据库服务器端取消失败或超时");
            return "数据库服务器端取消失败或超时".into();
        }
        CANCELLED.into()
    }

    async fn cancel_backend(&self) -> Result<(), String> {
        let Some((pid, started, marker)) = &self.identity else {
            return Ok(());
        };
        // 独立控制连接避免业务池已满时取消本身也排队。沿用原连接的 TLS 和认证配置。
        let mut control = PgConnection::connect_with(&self.pool.connect_options())
            .await
            .map_err(|_| "数据库取消连接失败".to_string())?;
        // SQLx 没有公开 CancelRequest。仅终止本操作保留的连接，以同时丢弃尚未发送完的命令。
        // PID、后端启动时间和事务内随机标记共同确认身份，不按 Scope 或用户名批量取消。
        let result = sqlx::query_scalar::<_, bool>(
            "SELECT pg_terminate_backend(pid, 5000) FROM pg_stat_activity WHERE pid = $1 AND backend_start = $2 AND application_name = $3",
        )
        .bind(pid)
        .bind(started)
        .bind(marker)
        .fetch_optional(&mut control)
        .await;
        let _ = control.close().await;
        match result {
            Ok(Some(false)) | Err(_) => Err("数据库服务器端取消失败".into()),
            Ok(_) => Ok(()),
        }
    }

    pub async fn commit(mut self) -> Result<(), String> {
        self.query(|connection| {
            Box::pin(async move { sqlx::query("COMMIT").execute(connection).await })
        })
        .await?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for PostgresOperation {
    fn drop(&mut self) {
        if !self.finished {
            // 错误、取消及外层停机期限耗尽都不可复用仍可能有命令在途的连接。
            self.connection.close_on_drop();
        }
    }
}
