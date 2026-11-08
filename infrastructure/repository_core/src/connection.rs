//! 统一构造连接选项；各入口明确提供自己的默认值。
use common::database_settings::DatabasePoolSettings;
use sea_orm::ConnectOptions;
use std::time::Duration;

pub struct PostgresPoolOptions {
    pub test_before_acquire: bool,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
    /// None 保留驱动默认行为，不能擅自套用 WebAPI 的默认值。
    pub acquire_timeout_seconds: Option<u64>,
    pub max_lifetime_seconds: Option<u64>,
    pub sqlx_logging: bool,
}
impl From<&DatabasePoolSettings> for PostgresPoolOptions {
    fn from(pool: &DatabasePoolSettings) -> Self {
        Self {
            test_before_acquire: pool.test_before_acquire,
            max_connections: pool.max_connections,
            min_connections: pool.min_connections,
            connect_timeout_seconds: pool.connect_timeout_seconds,
            idle_timeout_seconds: pool.idle_timeout_seconds,
            acquire_timeout_seconds: Some(pool.acquire_timeout_seconds),
            max_lifetime_seconds: Some(pool.max_lifetime_seconds),
            sqlx_logging: pool.sqlx_logging,
        }
    }
}
pub fn build_postgres_connect_options(
    connection: impl Into<String>,
    pool: impl Into<PostgresPoolOptions>,
) -> ConnectOptions {
    let pool = pool.into();
    let mut options = ConnectOptions::new(connection.into());
    options
        .test_before_acquire(pool.test_before_acquire)
        .max_connections(pool.max_connections)
        .min_connections(pool.min_connections)
        .connect_timeout(Duration::from_secs(pool.connect_timeout_seconds))
        .idle_timeout(Duration::from_secs(pool.idle_timeout_seconds))
        .sqlx_logging(pool.sqlx_logging);
    if let Some(seconds) = pool.acquire_timeout_seconds {
        options.acquire_timeout(Duration::from_secs(seconds));
    }
    if let Some(seconds) = pool.max_lifetime_seconds {
        options.max_lifetime(Duration::from_secs(seconds));
    }
    options
}
