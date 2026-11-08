use crate::startup::HostSettings;
use identifier_generator::IdentifierGeneratorSettings;
use logger::LoggerSettings;
use serde::Deserialize;

/// 任务宿主配置档案
pub struct TaskServiceHostProfile {
    pub build_databases: crate::BuildTaskServiceDatabases,
    /// 宿主名称
    pub host_name: &'static str,

    /// 默认配置路径
    pub default_config_path: &'static str,

    /// 默认锁前缀
    pub default_lock_prefix: &'static str,

    /// 默认日志项目名
    pub default_logger_project: &'static str,

    /// 默认文件根目录
    pub default_file_root: &'static str,

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

/// 文件存储组件类型
#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum FileStorageComponent {
    /// 本地文件存储
    Local,

    /// 阿里云 OSS 文件存储
    AliyunOss,

    /// 腾讯云 COS 文件存储
    TencentCos,
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

/// 文件存储配置
#[derive(Deserialize, Clone)]
pub struct FileSettings {
    /// 文件存储组件类型
    #[serde(default = "default_file_storage_component")]
    pub component: FileStorageComponent,

    /// 文件根目录
    #[serde(default = "default_file_root_path")]
    pub root_path: String,

    /// 对外访问基地址
    #[serde(default)]
    pub public_base_url: String,

    /// 阿里云 OSS 配置
    #[serde(default)]
    pub aliyun_oss: AliyunOssFileStorageSettings,

    /// 腾讯云 COS 配置
    #[serde(default)]
    pub tencent_cos: TencentCosFileStorageSettings,
}

/// 短信组件类型
#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum SmsComponent {
    /// 阿里云短信组件
    Aliyun,

    /// 腾讯云短信组件
    Tencent,
}

/// 短信配置
#[derive(Deserialize, Clone)]
pub struct SmsSettings {
    /// 短信组件类型
    #[serde(default = "default_sms_component")]
    pub component: SmsComponent,

    /// 阿里云配置
    #[serde(default)]
    pub aliyun: AliyunSmsSettings,

    /// 腾讯云配置
    #[serde(default)]
    pub tencent: TencentSmsSettings,
}

/// 邮件配置
#[derive(Deserialize, Clone)]
pub struct MessageSettings {
    /// SMTP 配置
    #[serde(default)]
    pub smtp: SmtpSettings,
}

/// SMTP 配置
#[derive(Deserialize, Clone)]
pub struct SmtpSettings {
    /// SMTP 主机地址
    #[serde(default)]
    pub host: String,

    /// SMTP 端口
    #[serde(default = "default_smtp_port")]
    pub port: u16,

    /// 用户名
    #[serde(default)]
    pub username: String,

    /// 密码
    #[serde(default)]
    pub password: String,
}

/// 阿里云 OSS 文件存储配置
#[derive(Deserialize, Clone)]
pub struct AliyunOssFileStorageSettings {
    /// OSS 存储区域
    #[serde(default)]
    pub region: String,

    /// 是否使用内网节点
    #[serde(default)]
    pub use_internal_endpoint: bool,

    /// AccessKeyId
    #[serde(default)]
    pub access_key_id: String,

    /// AccessKeySecret
    #[serde(default)]
    pub access_key_secret: String,

    /// 存储桶名称
    #[serde(default)]
    pub bucket_name: String,

    /// 自定义请求基地址
    #[serde(default)]
    pub endpoint: String,
}

/// 腾讯云 COS 文件存储配置
#[derive(Deserialize, Clone)]
pub struct TencentCosFileStorageSettings {
    /// 腾讯云账户 AppId
    #[serde(default)]
    pub app_id: String,

    /// COS 存储区域
    #[serde(default)]
    pub region: String,

    /// SecretId
    #[serde(default)]
    pub secret_id: String,

    /// SecretKey
    #[serde(default)]
    pub secret_key: String,

    /// 存储桶名称
    #[serde(default)]
    pub bucket_name: String,

    /// 自定义请求基地址
    #[serde(default)]
    pub endpoint: String,
}

/// 阿里云短信配置
#[derive(Deserialize, Clone)]
pub struct AliyunSmsSettings {
    /// 访问密钥标识
    #[serde(default)]
    pub access_key_id: String,

    /// 访问密钥内容
    #[serde(default)]
    pub access_key_secret: String,
}

/// 腾讯云短信配置
#[derive(Deserialize, Clone)]
pub struct TencentSmsSettings {
    /// 短信应用标识
    #[serde(default)]
    pub app_id: String,

    /// 密钥标识
    #[serde(default)]
    pub secret_id: String,

    /// 密钥内容
    #[serde(default)]
    pub secret_key: String,
}

/// 全局应用配置
#[derive(Deserialize, Clone)]
pub struct Settings {
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

    /// 文件配置
    #[serde(default)]
    pub file: FileSettings,

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

    /// 短信配置
    #[serde(default)]
    pub sms: SmsSettings,

    /// 邮件配置
    #[serde(default)]
    pub message: MessageSettings,
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

impl Default for FileSettings {
    /// 创建默认文件配置
    fn default() -> Self {
        Self {
            component: default_file_storage_component(),
            root_path: default_file_root_path(),
            public_base_url: String::new(),
            aliyun_oss: AliyunOssFileStorageSettings::default(),
            tencent_cos: TencentCosFileStorageSettings::default(),
        }
    }
}

impl Default for SmsSettings {
    /// 创建默认短信配置
    fn default() -> Self {
        Self {
            component: default_sms_component(),
            aliyun: AliyunSmsSettings::default(),
            tencent: TencentSmsSettings::default(),
        }
    }
}

impl Default for MessageSettings {
    /// 创建默认邮件配置
    fn default() -> Self {
        Self {
            smtp: SmtpSettings::default(),
        }
    }
}

impl Default for SmtpSettings {
    /// 创建默认 SMTP 配置
    fn default() -> Self {
        Self {
            host: String::new(),
            port: default_smtp_port(),
            username: String::new(),
            password: String::new(),
        }
    }
}

impl Default for AliyunOssFileStorageSettings {
    /// 创建默认阿里云 OSS 文件存储配置
    fn default() -> Self {
        Self {
            region: String::new(),
            use_internal_endpoint: false,
            access_key_id: String::new(),
            access_key_secret: String::new(),
            bucket_name: String::new(),
            endpoint: String::new(),
        }
    }
}

impl Default for TencentCosFileStorageSettings {
    /// 创建默认腾讯云 COS 文件存储配置
    fn default() -> Self {
        Self {
            app_id: String::new(),
            region: String::new(),
            secret_id: String::new(),
            secret_key: String::new(),
            bucket_name: String::new(),
            endpoint: String::new(),
        }
    }
}

impl Default for AliyunSmsSettings {
    /// 创建默认阿里云短信配置
    fn default() -> Self {
        Self {
            access_key_id: String::new(),
            access_key_secret: String::new(),
        }
    }
}

impl Default for TencentSmsSettings {
    /// 创建默认腾讯云短信配置
    fn default() -> Self {
        Self {
            app_id: String::new(),
            secret_id: String::new(),
            secret_key: String::new(),
        }
    }
}

impl Settings {
    /// 加载配置，默认从 appsettings.toml 读取
    pub fn load(profile: &TaskServiceHostProfile) -> Self {
        let config_path = std::env::var("APP_CONFIG_PATH")
            .unwrap_or_else(|_| profile.default_config_path.to_string());

        let environment_name = resolve_config_environment();
        let (configuration, reload_source) =
            logger::load_host_configuration(&config_path, Some(&environment_name))
                .expect("配置文件加载失败，请检查 appsettings.toml 是否存在");
        let mut settings: Self = configuration
            .try_deserialize()
            .expect("配置文件解析失败，请检查字段格式是否正确");

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

        if settings.file.root_path.trim().is_empty()
            || settings.file.root_path == default_file_root_path()
        {
            settings.file.root_path = profile.default_file_root.to_string();
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

/// 获取默认文件存储组件类型
fn default_file_storage_component() -> FileStorageComponent {
    FileStorageComponent::Local
}

fn resolve_config_environment() -> String {
    ["APP_ENVIRONMENT", "DOTNET_ENVIRONMENT"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| "Production".to_string())
}

/// 获取默认短信组件类型
fn default_sms_component() -> SmsComponent {
    SmsComponent::Aliyun
}

/// 获取默认 SMTP 端口
fn default_smtp_port() -> u16 {
    587
}

/// 获取默认文件根目录
fn default_file_root_path() -> String {
    "storage".to_string()
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
