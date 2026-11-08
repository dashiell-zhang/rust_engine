//! 数据库连接的纯配置类型；不持有连接或引用数据库驱动。
use serde::Deserialize;

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

    /// 连接超时时间（秒）
    #[serde(default = "default_connect_timeout_seconds")]
    pub connect_timeout_seconds: u64,

    /// 从连接池获取连接的超时时间（秒）
    #[serde(default = "default_acquire_timeout_seconds")]
    pub acquire_timeout_seconds: u64,

    /// 空闲超时时间（秒）
    #[serde(default = "default_idle_timeout_seconds")]
    pub idle_timeout_seconds: u64,

    /// 单个连接最大存活时间（秒）
    #[serde(default = "default_max_lifetime_seconds")]
    pub max_lifetime_seconds: u64,

    /// 是否输出 SQL 日志
    #[serde(default = "default_sqlx_logging")]
    pub sqlx_logging: bool,
}

impl Default for DatabasePoolSettings {
    /// 创建默认数据库连接池配置
    fn default() -> Self {
        Self {
            test_before_acquire: default_test_before_acquire(),
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            connect_timeout_seconds: default_connect_timeout_seconds(),
            acquire_timeout_seconds: default_acquire_timeout_seconds(),
            idle_timeout_seconds: default_idle_timeout_seconds(),
            max_lifetime_seconds: default_max_lifetime_seconds(),
            sqlx_logging: default_sqlx_logging(),
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
    5
}
/// 获取默认连接超时秒数
fn default_connect_timeout_seconds() -> u64 {
    10
}
/// 获取默认取连接超时秒数
fn default_acquire_timeout_seconds() -> u64 {
    5
}
/// 获取默认空闲超时秒数
fn default_idle_timeout_seconds() -> u64 {
    300
}
/// 获取默认连接最大存活秒数
fn default_max_lifetime_seconds() -> u64 {
    60 * 30
}
/// 获取默认 SQL 日志开关
fn default_sqlx_logging() -> bool {
    false
}

#[derive(Clone, serde::Deserialize)]
pub struct ModuleConnectionSettings {
    pub connection: String,
    #[serde(default)]
    pub read_connection: String,
}
impl ModuleConnectionSettings {
    pub fn validate(&self, module: &str) -> Result<(), String> {
        if self.connection.trim().is_empty() {
            return Err(format!("数据库模块 {module} 未配置写连接"));
        }
        Ok(())
    }
    pub fn read_connection(&self) -> &str {
        if self.read_connection.trim().is_empty() {
            &self.connection
        } else {
            &self.read_connection
        }
    }
}
