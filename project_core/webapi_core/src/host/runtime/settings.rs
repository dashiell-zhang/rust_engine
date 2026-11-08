use crate::{rate_limit::RateLimitSettings, startup::HostSettings};
use identifier_generator::IdentifierGeneratorSettings;
use logger::LoggerSettings;
use serde::Deserialize;

use crate::WebApiHostProfile;

/// 锁组件类型
#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum LockComponent {
    /// 进程内锁
    InMemory,

    /// Redis 分布式锁
    Redis,
}

/// 缓存组件类型
#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CacheComponent {
    /// 进程内缓存
    InMemory,

    /// Redis 缓存
    Redis,
}

/// 数据库配置
#[derive(Deserialize, Clone, Default)]
pub struct DatabaseSettings {
    /// 显式模块目标；已声明但缺少写连接时不回退默认库。
    #[serde(default)]
    pub modules:
        std::collections::BTreeMap<String, common::database_settings::ModuleConnectionSettings>,
    /// PostgreSQL 连接字符串
    #[serde(default)]
    pub connection: String,

    /// PostgreSQL 只读连接字符串 为空时回退主库连接字符串
    #[serde(default)]
    pub read_connection: String,
}

/// JWT 配置
#[derive(Deserialize, Clone)]
pub struct JwtSettings {
    /// Token 颁发者
    pub issuer: String,
    /// Token 受众
    pub audience: String,
    /// ECDSA 私钥（Base64 编码），优先支持 PKCS8 DER，也兼容 SEC1 DER
    pub private_key: String,
    /// ECDSA SPKI DER 公钥（Base64 编码），用于验签
    pub public_key: String,
    /// Token 有效期（秒）
    pub expiry_seconds: i64,
}

/// 锁配置
#[derive(Deserialize, Clone)]
pub struct LockSettings {
    /// 锁组件类型
    #[serde(default = "default_lock_component")]
    pub component: LockComponent,

    /// Redis 连接字符串
    #[serde(default)]
    pub connection: String,

    /// 分布式锁前缀
    #[serde(default = "default_lock_prefix")]
    pub prefix: String,
}

/// 旧版 Redis 锁配置
#[derive(Deserialize, Clone, Default)]
pub struct LegacyRedisSettings {
    /// Redis 连接字符串
    #[serde(default)]
    pub connection: String,

    /// 分布式锁前缀
    #[serde(default)]
    pub lock_prefix: String,
}

/// 文件存储配置
#[derive(Deserialize, Clone)]
pub struct FileSettings {
    /// 文件根目录
    #[serde(default = "default_file_root_path")]
    pub root_path: String,

    /// 对外访问基地址
    #[serde(default)]
    pub public_base_url: String,
}

/// 缓存配置
#[derive(Deserialize, Clone)]
pub struct CacheSettings {
    /// 缓存组件类型
    #[serde(default = "default_cache_component")]
    pub component: CacheComponent,

    /// Redis 连接字符串
    #[serde(default)]
    pub connection: String,

    /// 缓存前缀
    #[serde(default = "default_cache_prefix")]
    pub prefix: String,
}

/// CORS 配置
#[derive(Deserialize, Clone)]
pub struct CorsSettings {
    /// 允许的来源列表
    #[serde(default = "default_cors_allowed_origin_list")]
    pub allowed_origin_list: Vec<String>,
}

/// 全局应用配置
#[derive(Clone)]
pub struct Settings {
    /// 已按宿主规则合并的启动快照；具体业务配置由入口解析。
    configuration: config::Config,
    /// 宿主公共配置
    pub host: HostSettings,
    /// 权限模块名
    pub permission_module: String,
    /// 数据库配置
    pub database: DatabaseSettings,
    /// JWT 配置
    pub jwt: JwtSettings,
    /// 按需启用的请求字段解密配置
    pub rsa: Option<crate::rsa_decrypt::RsaSettings>,
    /// 锁配置
    pub lock: LockSettings,
    /// 日志配置
    pub logger: LoggerSettings,
    /// 跨进程广播配置
    pub event_bus: event_bus::EventBusOptions,
    /// 文件配置
    pub file: FileSettings,
    /// 缓存配置
    pub cache: CacheSettings,
    /// CORS 配置
    pub cors: CorsSettings,
    /// 标识生成器配置
    pub identifier_generator: IdentifierGeneratorSettings,
    /// 限流配置
    pub rate_limit: RateLimitSettings,
    /// 运行入口负责解析的命名 HTTP 客户端配置。
    pub http_clients: std::collections::HashMap<String, serde_json::Value>,
}

/// 原始全局应用配置
#[derive(Deserialize)]
struct RawSettings {
    /// 宿主公共配置
    #[serde(default)]
    pub host: HostSettings,
    /// 权限模块名
    #[serde(default)]
    pub permission_module: String,
    /// 数据库配置
    #[serde(default)]
    pub database: DatabaseSettings,
    /// JWT 配置
    pub jwt: JwtSettings,
    /// 按需启用的请求字段解密配置
    #[serde(default)]
    pub rsa: Option<crate::rsa_decrypt::RsaSettings>,
    /// 锁配置
    pub lock: Option<LockSettings>,
    /// 旧版 Redis 锁配置
    #[serde(default)]
    pub redis: LegacyRedisSettings,
    /// 日志配置
    #[serde(default)]
    pub logger: LoggerSettings,
    /// 跨进程广播配置
    #[serde(default)]
    pub event_bus: event_bus::EventBusOptions,
    /// 文件配置
    #[serde(default)]
    pub file: FileSettings,
    /// 缓存配置
    pub cache: Option<CacheSettings>,
    /// CORS 配置
    #[serde(default)]
    pub cors: CorsSettings,
    /// 标识生成器配置
    #[serde(default)]
    pub identifier_generator: IdentifierGeneratorSettings,
    /// 限流配置
    #[serde(default)]
    pub rate_limit: RateLimitSettings,
    /// 运行入口负责解析的命名 HTTP 客户端配置。
    #[serde(default)]
    pub http_clients: std::collections::HashMap<String, serde_json::Value>,
}

impl Default for LockSettings {
    /// 创建默认锁配置
    fn default() -> Self {
        Self {
            component: default_lock_component(),
            connection: String::new(),
            prefix: default_lock_prefix(),
        }
    }
}

impl Default for FileSettings {
    /// 创建默认文件配置
    fn default() -> Self {
        Self {
            root_path: default_file_root_path(),
            public_base_url: String::new(),
        }
    }
}

impl Default for CacheSettings {
    /// 创建默认缓存配置
    fn default() -> Self {
        Self {
            component: default_cache_component(),
            connection: String::new(),
            prefix: default_cache_prefix(),
        }
    }
}

impl Default for CorsSettings {
    /// 创建默认 CORS 配置
    fn default() -> Self {
        Self {
            allowed_origin_list: default_cors_allowed_origin_list(),
        }
    }
}

impl Settings {
    /// 从同一启动快照解析入口拥有的配置，不重新读取文件或环境变量。
    pub fn extension<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        self.configuration.clone().try_deserialize().map_err(|_| {
            format!(
                "扩展配置 {} 格式不正确，请检查配置字段类型",
                std::any::type_name::<T>()
            )
        })
    }

    /// 加载配置，默认从 appsettings.toml 读取
    ///
    /// 配置文件路径优先级
    /// 1. 环境变量 `APP_CONFIG_PATH` 指定的路径
    /// 2. 宿主档案指定的默认路径（适配从 workspace 根目录运行）
    /// 3. 按共享宿主环境追加加载 `appsettings.{环境名}.toml`，默认环境为 Production
    ///
    /// 所有配置项均可通过 `APP__节名__键名` 格式的环境变量覆盖 例如
    /// `APP__DATABASE__CONNECTION=postgres://...`
    pub fn load(profile: &WebApiHostProfile) -> Self {
        // 支持通过环境变量覆盖配置文件路径 方便不同部署环境切换
        let config_path = std::env::var("APP_CONFIG_PATH")
            .unwrap_or_else(|_| profile.default_config_path.to_string());

        let environment_name = crate::environment::name();
        let (configuration, reload_source) =
            logger::load_host_configuration(&config_path, Some(environment_name))
                .expect("配置文件加载失败，请检查 appsettings.toml 是否存在");
        let raw_settings: RawSettings = configuration
            .clone()
            .try_deserialize()
            .expect("配置文件解析失败，请检查字段格式是否正确");

        // 优先使用新锁配置 旧配置仅用于补全缺失字段
        let mut lock = raw_settings.lock.unwrap_or_default();

        // 兼容旧版 redis 节点中的锁连接配置
        if lock.connection.trim().is_empty() && !raw_settings.redis.connection.trim().is_empty() {
            lock.connection = raw_settings.redis.connection.clone();
        }

        if lock.prefix == default_lock_prefix() && !raw_settings.redis.lock_prefix.trim().is_empty()
        {
            lock.prefix = raw_settings.redis.lock_prefix.clone();
        }

        if lock.prefix == default_lock_prefix() {
            lock.prefix = profile.default_lock_prefix.to_string();
        }

        // 汇总新旧配置后再填充与宿主相关的默认值
        let mut settings = Self {
            configuration,
            host: raw_settings.host,
            permission_module: raw_settings.permission_module,
            database: raw_settings.database,
            jwt: raw_settings.jwt,
            rsa: raw_settings.rsa,
            lock,
            logger: raw_settings.logger,
            event_bus: raw_settings.event_bus,
            file: raw_settings.file,
            cache: raw_settings.cache.unwrap_or_default(),
            cors: raw_settings.cors,
            identifier_generator: raw_settings.identifier_generator,
            rate_limit: raw_settings.rate_limit,
            http_clients: raw_settings.http_clients,
        };

        settings.logger.set_filter_reload_source(reload_source);

        // 这些默认值依赖宿主档案 因此统一在最终阶段补齐
        if settings.logger.project.trim().is_empty() {
            settings.logger.project = profile.default_logger_project.to_string();
        }

        if settings.permission_module.trim().is_empty() {
            settings.permission_module = profile.permission_module.to_string();
        }

        if settings.file.root_path.trim().is_empty()
            || settings.file.root_path == default_file_root_path()
        {
            settings.file.root_path = profile.default_file_root.to_string();
        }

        if settings.cache.prefix.trim().is_empty()
            || settings.cache.prefix == default_cache_prefix()
        {
            settings.cache.prefix = profile.host_name.to_string();
        }

        if let Some(validate) = profile.validate_configuration {
            validate(&settings).expect("业务组件配置解析失败");
        }
        settings
    }
}

/// 获取默认锁组件类型
fn default_lock_component() -> LockComponent {
    LockComponent::InMemory
}

/// 获取默认锁前缀
fn default_lock_prefix() -> String {
    "webapi".to_string()
}

/// 获取默认文件根目录
fn default_file_root_path() -> String {
    "data/files".to_string()
}

/// 获取默认缓存组件类型
fn default_cache_component() -> CacheComponent {
    CacheComponent::InMemory
}

/// 获取默认缓存前缀
fn default_cache_prefix() -> String {
    "webapi".to_string()
}

/// 获取默认允许的跨域来源列表
fn default_cors_allowed_origin_list() -> Vec<String> {
    vec!["*.xxx.com".to_string()]
}
