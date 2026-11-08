use crate::startup::HostSettings;
use identifier_generator::IdentifierGeneratorSettings;
use logger::LoggerSettings;
use serde::Deserialize;

/// 入口拥有的配置校验，不绑定具体业务组件。
pub type ValidateConfiguration = fn(&Settings) -> Result<(), String>;

/// 任务宿主配置档案
pub struct TaskServiceHostProfile {
    /// 入口在任何资源连接建立前验证自己的扩展配置。
    pub validate_configuration: Option<ValidateConfiguration>,
    pub build_databases: crate::BuildTaskServiceDatabases,
    /// 宿主名称
    pub host_name: &'static str,

    /// 默认配置路径
    pub default_config_path: &'static str,

    /// 默认锁前缀
    pub default_lock_prefix: &'static str,

    /// 默认日志项目名
    pub default_logger_project: &'static str,

    /// 分布式锁构建方法
    pub build_distributed_lock: crate::BuildTaskServiceLock,

    /// 分布式缓存构建方法
    pub build_distributed_cache: crate::BuildTaskServiceCache,

    /// 日志运行时构建方法
    pub build_logger_runtime: crate::BuildTaskServiceLoggerRuntime,
    pub build_event_transport: crate::BuildTaskServiceEventTransport,

    /// 任务业务资源由运行入口按需装配。
    pub build_resources: Option<crate::BuildTaskServiceResources>,
}

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
        std::collections::BTreeMap<String, repository_core::module::ModuleConnectionSettings>,
    /// PostgreSQL 连接字符串
    #[serde(default)]
    pub connection: String,

    /// PostgreSQL 只读连接字符串 为空时回退主库连接字符串
    #[serde(default)]
    pub read_connection: String,
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

    /// 锁前缀
    #[serde(default = "default_lock_prefix")]
    pub prefix: String,

    /// 锁过期秒数
    #[serde(default = "default_lock_expiry_seconds")]
    pub expiry_seconds: u64,
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

/// 任务轮询配置
#[derive(Deserialize, Clone)]
pub struct PollSettings {
    /// 队列任务轮询间隔秒数
    #[serde(default = "default_queue_interval_seconds")]
    pub queue_interval_seconds: u64,

    /// 兼容旧版的定时任务轮询间隔秒数
    #[serde(default)]
    pub schedule_interval_seconds: Option<u64>,

    /// 定时任务轮询间隔毫秒数
    #[serde(default = "default_schedule_interval_milliseconds")]
    pub schedule_interval_milliseconds: u64,
}

/// 全局应用配置
#[derive(Clone)]
pub struct Settings {
    /// 已按宿主规则合并的启动快照；具体业务配置由入口解析。
    configuration: config::Config,
    /// 宿主公共配置
    pub host: HostSettings,

    /// 数据库配置
    pub database: DatabaseSettings,

    /// 锁配置
    pub lock: LockSettings,
    /// 缓存配置
    pub cache: CacheSettings,

    /// 标识生成器配置
    pub identifier_generator: IdentifierGeneratorSettings,

    /// 轮询配置
    pub poll: PollSettings,

    /// 日志配置
    pub logger: LoggerSettings,
    /// 跨进程广播配置
    pub event_bus: event_bus::EventBusOptions,
}

/// 仅用于读取公共配置字段；具体组件字段留在原始快照。
#[derive(Deserialize, Clone)]
struct RawSettings {
    /// 宿主公共配置
    #[serde(default)]
    pub host: HostSettings,

    /// 数据库配置
    #[serde(default)]
    pub database: DatabaseSettings,

    /// 锁配置
    #[serde(default)]
    pub lock: LockSettings,
    /// 缓存配置
    #[serde(default)]
    pub cache: CacheSettings,

    /// 标识生成器配置
    #[serde(default)]
    pub identifier_generator: IdentifierGeneratorSettings,

    /// 轮询配置
    #[serde(default)]
    pub poll: PollSettings,

    /// 日志配置
    #[serde(default)]
    pub logger: LoggerSettings,
    /// 跨进程广播配置
    #[serde(default)]
    pub event_bus: event_bus::EventBusOptions,
}

impl Default for PollSettings {
    /// 创建默认轮询配置
    fn default() -> Self {
        Self {
            queue_interval_seconds: default_queue_interval_seconds(),
            schedule_interval_seconds: None,
            schedule_interval_milliseconds: default_schedule_interval_milliseconds(),
        }
    }
}

impl PollSettings {
    /// 获取最终定时任务轮询间隔毫秒数
    pub fn resolved_schedule_interval_milliseconds(&self) -> u64 {
        if self.schedule_interval_milliseconds > 0 {
            return self.schedule_interval_milliseconds;
        }

        self.schedule_interval_seconds
            .filter(|value| *value > 0)
            .map(|value| value.saturating_mul(1000))
            .unwrap_or_else(default_schedule_interval_milliseconds)
    }
}

impl Default for LockSettings {
    /// 创建默认锁配置
    fn default() -> Self {
        Self {
            component: default_lock_component(),
            connection: String::new(),
            prefix: default_lock_prefix(),
            expiry_seconds: default_lock_expiry_seconds(),
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
    pub fn load(profile: &TaskServiceHostProfile) -> Self {
        let config_path = std::env::var("APP_CONFIG_PATH")
            .unwrap_or_else(|_| profile.default_config_path.to_string());

        let environment_name = resolve_config_environment();
        let (configuration, reload_source) =
            logger::load_host_configuration(&config_path, Some(&environment_name))
                .expect("配置文件加载失败，请检查 appsettings.toml 是否存在");
        let raw: RawSettings = configuration
            .clone()
            .try_deserialize()
            .expect("配置文件解析失败，请检查字段格式是否正确");

        let mut settings = Self {
            configuration,
            host: raw.host,
            database: raw.database,
            lock: raw.lock,
            cache: raw.cache,
            identifier_generator: raw.identifier_generator,
            poll: raw.poll,
            logger: raw.logger,
            event_bus: raw.event_bus,
        };
        settings.logger.set_filter_reload_source(reload_source);
        if settings.logger.project.trim().is_empty() {
            settings.logger.project = profile.default_logger_project.to_string();
        }

        if settings.lock.prefix.trim().is_empty() || settings.lock.prefix == default_lock_prefix() {
            settings.lock.prefix = profile.default_lock_prefix.to_string();
        }

        if settings.cache.prefix.trim().is_empty()
            || settings.cache.prefix == default_cache_prefix()
        {
            settings.cache.prefix = profile.host_name.to_string();
        }

        if settings.poll.schedule_interval_seconds.is_some()
            && settings.poll.schedule_interval_milliseconds
                == default_schedule_interval_milliseconds()
        {
            settings.poll.schedule_interval_milliseconds = settings
                .poll
                .schedule_interval_seconds
                .filter(|value| *value > 0)
                .map(|value| value.saturating_mul(1000))
                .unwrap_or_else(default_schedule_interval_milliseconds);
        }

        if let Some(validate) = profile.validate_configuration {
            validate(&settings).expect("业务组件配置解析失败");
        }
        settings
    }
}

/// 获取默认队列轮询间隔秒数
fn default_queue_interval_seconds() -> u64 {
    1
}

/// 获取默认定时轮询间隔毫秒数
fn default_schedule_interval_milliseconds() -> u64 {
    128
}

/// 获取默认锁组件类型
fn default_lock_component() -> LockComponent {
    LockComponent::InMemory
}

/// 获取默认缓存组件类型
fn default_cache_component() -> CacheComponent {
    CacheComponent::InMemory
}

fn resolve_config_environment() -> String {
    ["APP_ENVIRONMENT", "DOTNET_ENVIRONMENT"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "Production".to_string())
}

/// 获取默认锁前缀
fn default_lock_prefix() -> String {
    "task_service".to_string()
}

/// 获取默认缓存前缀
fn default_cache_prefix() -> String {
    "task_service".to_string()
}

/// 获取默认锁过期秒数
fn default_lock_expiry_seconds() -> u64 {
    300
}
