use base64::{Engine as _, engine::general_purpose};
use jsonwebtoken::{DecodingKey, EncodingKey};
use p256::{
    PublicKey, SecretKey,
    elliptic_curve::sec1::ToSec1Point,
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey},
};
use sea_orm::ConnectOptions;
use serde::Deserialize;
use std::time::Duration;
use tokio::net::TcpListener;

/// JWT 密钥对
pub struct JwtKeyPair {
    /// JWT 编码密钥
    pub encoding_key: EncodingKey,

    /// JWT 验签密钥
    pub decoding_key: DecodingKey,
}

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

/// 宿主配置
#[derive(Deserialize, Clone)]
pub struct HostSettings {
    /// 服务监听地址
    #[serde(default = "default_listen_address")]
    pub listen_address: String,

    /// 外部 HTTPS 重定向端口；未配置时读取 HTTPS_PORT 等宿主环境配置。
    #[serde(default)]
    pub https_port: Option<u16>,

    /// 可选直接 HTTPS 监听。
    #[serde(default)]
    pub tls: Option<crate::tls::TlsSettings>,

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
            acquire_timeout_seconds: default_acquire_timeout_seconds(),
            idle_timeout_seconds: default_idle_timeout_seconds(),
            max_lifetime_seconds: default_max_lifetime_seconds(),
            sqlx_logging: default_sqlx_logging(),
        }
    }
}

impl Default for HostSettings {
    /// 创建默认宿主配置
    fn default() -> Self {
        Self {
            listen_address: default_listen_address(),
            https_port: None,
            tls: None,
            database_pool: DatabasePoolSettings::default(),
        }
    }
}

/// WebAPI 启动帮助方法
pub struct WebApiStartup;

impl WebApiStartup {
    /// 解析 ECDSA JWT 密钥对
    pub fn parse_ec_jwt_key_pair(
        private_key: &str,
        public_key: &str,
    ) -> Result<JwtKeyPair, String> {
        let private_key_bytes = general_purpose::STANDARD
            .decode(private_key)
            .map_err(|error| format!("JWT private_key base64 解码失败: {error}"))?;
        let public_key_bytes = general_purpose::STANDARD
            .decode(public_key)
            .map_err(|error| format!("JWT public_key base64 解码失败: {error}"))?;

        let normalized_private_key_bytes = normalize_ec_private_key_der(&private_key_bytes)?;
        let normalized_public_key_bytes = normalize_ec_public_key_bytes(&public_key_bytes)?;

        Ok(JwtKeyPair {
            encoding_key: EncodingKey::from_ec_der(&normalized_private_key_bytes),
            decoding_key: DecodingKey::from_ec_der(&normalized_public_key_bytes),
        })
    }

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
            .acquire_timeout(Duration::from_secs(pool.acquire_timeout_seconds))
            .idle_timeout(Duration::from_secs(pool.idle_timeout_seconds))
            .max_lifetime(Duration::from_secs(pool.max_lifetime_seconds))
            .sqlx_logging(pool.sqlx_logging);

        options
    }

    /// 绑定 TCP 监听地址
    pub async fn bind_listener(address: &str) -> Result<TcpListener, String> {
        TcpListener::bind(address)
            .await
            .map_err(|error| format!("监听地址绑定失败: {error}"))
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

/// 归一化 ECDSA 公钥字节，兼容 SPKI DER 与 SEC1 未压缩点格式
fn normalize_ec_public_key_bytes(public_key_bytes: &[u8]) -> Result<Vec<u8>, String> {
    if public_key_bytes.len() == 65 && public_key_bytes.first() == Some(&0x04) {
        return Ok(public_key_bytes.to_vec());
    }

    let public_key = PublicKey::from_public_key_der(public_key_bytes)
        .map_err(|error| format!("JWT public_key 解析失败: {error}"))?;

    Ok(public_key.to_sec1_point(false).as_bytes().to_vec())
}

/// 获取默认 SQL 日志开关
fn default_sqlx_logging() -> bool {
    false
}

/// 获取默认监听地址
fn default_listen_address() -> String {
    "0.0.0.0:3000".to_string()
}

fn normalize_ec_private_key_der(private_key_bytes: &[u8]) -> Result<Vec<u8>, String> {
    if let Ok(secret_key) = SecretKey::from_pkcs8_der(private_key_bytes) {
        return secret_key
            .to_pkcs8_der()
            .map(|document| document.as_bytes().to_vec())
            .map_err(|error| format!("JWT private_key PKCS8 编码失败: {error}"));
    }

    if let Ok(secret_key) = SecretKey::from_sec1_der(private_key_bytes) {
        return secret_key
            .to_pkcs8_der()
            .map(|document| document.as_bytes().to_vec())
            .map_err(|error| format!("JWT private_key SEC1 转 PKCS8 失败: {error}"));
    }

    Err("JWT private_key 既不是有效的 PKCS8 DER，也不是有效的 SEC1 DER".to_string())
}
