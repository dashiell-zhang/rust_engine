use std::io;

use sea_orm::ConnectOptions;
use serde::Deserialize;
use std::time::Duration;

/// 数据库连接池配置
#[derive(Deserialize, Clone)]
pub struct DatabasePoolSettings {
    /// 是否在取连接前执行健康检查
    #[serde(default = "default_test_before_acquire")]
    pub test_before_acquire: bool,

    /// 最大连接数
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// 最小连接数
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,

    /// 连接超时时间
    #[serde(default = "default_connect_timeout_seconds")]
    pub connect_timeout_seconds: u64,

    /// 空闲超时时间
    #[serde(default = "default_idle_timeout_seconds")]
    pub idle_timeout_seconds: u64,

    /// 是否输出 SQL 日志
    #[serde(default = "default_sqlx_logging")]
    pub sqlx_logging: bool,
}

/// 宿主配置
#[derive(Deserialize, Clone)]
pub struct HostSettings {
    /// 数据库连接池配置
    #[serde(default)]
    pub database_pool: DatabasePoolSettings,
}

impl Default for DatabasePoolSettings {
    /// 创建默认数据库连接池配置
    fn default() -> Self {
        Self {
            test_before_acquire: default_test_before_acquire(),
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            connect_timeout_seconds: default_connect_timeout_seconds(),
            idle_timeout_seconds: default_idle_timeout_seconds(),
            sqlx_logging: default_sqlx_logging(),
        }
    }
}

impl Default for HostSettings {
    /// 创建默认宿主配置
    fn default() -> Self {
        Self {
            database_pool: DatabasePoolSettings::default(),
        }
    }
}

/// 任务宿主启动帮助方法
pub struct TaskServiceStartup;

impl TaskServiceStartup {
    /// 创建默认 PostgreSQL 连接配置
    pub fn build_postgres_connect_options(
        connection: impl Into<String>,
        pool: &DatabasePoolSettings,
    ) -> ConnectOptions {
        let mut options = ConnectOptions::new(connection.into());

        options.test_before_acquire(pool.test_before_acquire);

        options
            .max_connections(pool.max_connections)
            .min_connections(pool.min_connections)
            .connect_timeout(Duration::from_secs(pool.connect_timeout_seconds))
            .idle_timeout(Duration::from_secs(pool.idle_timeout_seconds))
            .sqlx_logging(pool.sqlx_logging);

        options
    }

    /// Debug 模式下等待人工确认是否启用任务宿主
    pub fn wait_for_debug_enable() -> bool {
        if !cfg!(debug_assertions) {
            return true;
        }

        println!("TaskService Debug 模式");
        println!("输入 Y 或直接回车启动任务宿主，输入 N 跳过");

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let text = input.trim().to_ascii_lowercase();
                text.is_empty() || text == "y" || text == "yes"
            }
            Err(_) => true,
        }
    }
}

/// 获取默认连接前检查开关
fn default_test_before_acquire() -> bool {
    false
}

/// 获取默认最大连接数
fn default_max_connections() -> u32 {
    30
}

/// 获取默认最小连接数
fn default_min_connections() -> u32 {
    30
}

/// 获取默认连接超时秒数
fn default_connect_timeout_seconds() -> u64 {
    50
}

/// 获取默认空闲超时秒数
fn default_idle_timeout_seconds() -> u64 {
    300
}

/// 获取默认 SQL 日志开关
fn default_sqlx_logging() -> bool {
    false
}
