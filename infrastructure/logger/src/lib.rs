use std::{collections::HashMap, env, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Number, Value};
use tokio::task::JoinHandle as TokioJoinHandle;
use tracing::{
    Event, Id, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Record},
};
use tracing_subscriber::{
    Layer, fmt,
    layer::Context,
    layer::SubscriberExt,
    registry::{LookupSpan, Registry},
};

mod buffer_settings;
mod console;
mod filter;
mod filter_runtime;
mod level;
mod reload;
mod settings_source;
pub use buffer_settings::LogBufferSettings;
pub use filter_runtime::LoggerFilterHandle;
pub use reload::{LogFilterReloadSource, load_host_configuration};
pub use settings_source::{log_filter_environment, log_filter_source};

/// 日志输出目标，名称与源提供者类型及控制台别名对应。
#[derive(Clone, Copy)]
pub enum LoggerProvider {
    Console,
    LocalFile,
    Database,
}

impl LoggerProvider {
    fn names(self) -> &'static [&'static str] {
        match self {
            Self::Console => &[
                "Console",
                "Microsoft.Extensions.Logging.Console.ConsoleLoggerProvider",
            ],
            Self::LocalFile => &["Logger.LocalFile.LocalFileLoggerProvider"],
            Self::Database => &["Logger.Database.DatabaseLoggerProvider"],
        }
    }
}

#[derive(Deserialize, Clone, Default)]
pub struct LoggerProviderSettings {
    #[serde(default)]
    pub log_level: HashMap<String, String>,
}

/// 日志组件类型
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoggerComponent {
    /// 数据库日志
    Database,

    /// 本地文件日志
    LocalFile,
}

/// 日志配置
#[derive(Deserialize, Clone)]
pub struct LoggerSettings {
    /// 是否自动重新读取文件中的过滤规则；环境变量仍使用启动快照。
    #[serde(default = "default_reload_on_change")]
    pub reload_on_change: bool,
    #[serde(skip)]
    reload_source: Option<LogFilterReloadSource>,
    #[serde(default)]
    pub database_buffer: LogBufferSettings,
    #[serde(default)]
    pub local_file_buffer: LogBufferSettings,
    /// 日志维护锁连接，留空时选用宿主锁连接；所有访问同一日志库的实例应一致。
    #[serde(default)]
    pub maintenance_lock_connection: String,
    #[serde(default = "default_maintenance_lock_prefix")]
    pub maintenance_lock_prefix: String,
    /// 日志写库连接；为空时宿主显式选用业务库地址，但创建独立连接池。
    #[serde(default)]
    pub connection: String,

    /// 日志只读连接；为空时选用日志写库地址，不使用业务只读库。
    #[serde(default)]
    pub read_connection: String,

    /// 日志存储逻辑标识，用于隔离积压文件和维护任务。
    #[serde(default = "default_database_identity")]
    pub database_identity: String,
    /// 日志组件类型
    #[serde(default = "default_logger_component")]
    pub component: LoggerComponent,

    /// 日志级别配置，key 为不区分大小写的 tracing target 前缀或单通配符规则。
    /// default 表示默认级别，类别名称保留首尾空白。
    #[serde(default = "default_logger_log_level")]
    pub log_level: HashMap<String, String>,

    /// 提供者专属级别规则；未命中时才回退全局规则。
    #[serde(default)]
    pub providers: HashMap<String, LoggerProviderSettings>,

    /// 项目标识
    #[serde(default = "default_logger_project")]
    pub project: String,

    /// 日志保存天数
    #[serde(default = "default_logger_save_days")]
    pub save_days: i64,
}

impl Default for LoggerSettings {
    /// 创建默认日志配置
    fn default() -> Self {
        Self {
            database_buffer: LogBufferSettings::default(),
            reload_on_change: true,
            reload_source: None,
            local_file_buffer: LogBufferSettings::default(),
            maintenance_lock_connection: String::new(),
            maintenance_lock_prefix: default_maintenance_lock_prefix(),
            connection: String::new(),
            read_connection: String::new(),
            database_identity: default_database_identity(),
            component: default_logger_component(),
            log_level: default_logger_log_level(),
            providers: HashMap::new(),
            project: default_logger_project(),
            save_days: default_logger_save_days(),
        }
    }
}

fn default_database_identity() -> String {
    "RustEngine.Log".to_string()
}

fn default_reload_on_change() -> bool {
    true
}

fn default_maintenance_lock_prefix() -> String {
    "NetEngine:PartitionMaintenance:".to_string()
}

impl LoggerSettings {
    pub fn write_connection<'a>(&'a self, business_connection: &'a str) -> &'a str {
        if self.connection.trim().is_empty() {
            business_connection
        } else {
            &self.connection
        }
    }

    pub fn query_connection<'a>(&'a self, business_connection: &'a str) -> &'a str {
        if self.read_connection.trim().is_empty() {
            self.write_connection(business_connection)
        } else {
            &self.read_connection
        }
    }
}

/// 日志运行时守卫
///
/// 正常退出显式等待 shutdown 排空；Drop 只取消后台任务。
pub struct LoggerRuntime {
    async_writer: Option<Arc<dyn AsyncLogWriter>>,
    filter_handle: Option<LoggerFilterHandle>,
    filter_watcher: Option<reload::FilterWatcher>,
    /// 提供者持有的后台服务守卫，与日志运行时一同释放。
    background_guards: Vec<Box<dyn Send + Sync>>,
    /// 数据库日志入库任务句柄（仅 Database 组件使用）
    save_handle: Option<TokioJoinHandle<()>>,

    /// 日志清理任务句柄
    clear_handle: Option<TokioJoinHandle<()>>,
}

impl LoggerRuntime {
    /// 宿主完成配置加载后，为当前提供者启动文件过滤规则监听。
    pub fn with_filter_reload(mut self, settings: &LoggerSettings) -> Result<Self, String> {
        if settings.reload_on_change
            && let Some(source) = &settings.reload_source
        {
            let handle = self
                .filter_handle
                .as_ref()
                .ok_or_else(|| "日志运行时缺少过滤规则更新入口".to_string())?;
            self.filter_watcher = Some(reload::FilterWatcher::start(
                source.clone(),
                handle.clone(),
            )?);
        }
        Ok(self)
    }

    /// 将过滤规则更新入口交给持有提供者生命周期的日志运行时。
    pub fn with_filter_handle(mut self, handle: LoggerFilterHandle) -> Self {
        self.filter_handle = Some(handle);
        self
    }

    /// 只替换过滤规则，不修改日志存储、缓冲、连接或项目标识。
    pub fn reload_filter(&self, settings: &LoggerSettings) -> Result<(), String> {
        self.filter_handle
            .as_ref()
            .ok_or_else(|| "日志运行时没有活动的过滤规则更新入口".to_string())?
            .reload(settings)
    }

    pub fn with_background_guard(mut self, guard: impl Send + Sync + 'static) -> Self {
        self.background_guards.push(Box::new(guard));
        self
    }

    pub fn new_async(
        writer: Arc<dyn AsyncLogWriter>,
        save: Option<TokioJoinHandle<()>>,
        clear: Option<TokioJoinHandle<()>>,
    ) -> Self {
        Self {
            async_writer: Some(writer),
            filter_handle: None,
            filter_watcher: None,
            background_guards: Vec::new(),
            save_handle: save,
            clear_handle: clear,
        }
    }

    /// 正常停机先排空内存到文件，再停止入库和维护；无需等待积压全部入库。
    pub async fn shutdown(&mut self, timeout: std::time::Duration) {
        let deadline = tokio::time::Instant::now() + timeout;
        if let Some(watcher) = self.filter_watcher.take() {
            watcher.shutdown().await;
        }
        self.filter_handle.take();
        if let Some(writer) = &self.async_writer {
            writer
                .shutdown(deadline.saturating_duration_since(tokio::time::Instant::now()))
                .await;
        }
        self.async_writer.take();
        if let Some(handle) = self.save_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        if let Some(handle) = self.clear_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        self.background_guards.clear();
    }
}

/// 日志提供者自己的异步排空协议，不让公共日志层依赖数据库实现。
pub trait AsyncLogWriter: Send + Sync {
    fn shutdown(
        &self,
        timeout: std::time::Duration,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
    fn cancel(&self);
}

impl Drop for LoggerRuntime {
    /// 释放日志运行时资源
    fn drop(&mut self) {
        self.filter_watcher.take();
        if let Some(writer) = self.async_writer.take() {
            writer.cancel();
        }
        if let Some(handle) = self.save_handle.take() {
            handle.abort();
        }

        if let Some(handle) = self.clear_handle.take() {
            handle.abort();
        }
    }
}

/// 待写入日志
#[derive(Debug, Clone)]
pub struct PendingLog {
    /// 项目标识
    pub project: String,

    /// 机器名称
    pub machine_name: String,

    /// 日志级别
    pub level: String,

    /// 日志分类
    pub category: String,

    /// 日志内容
    pub content: String,

    /// 日志创建时间
    pub create_time: DateTime<Utc>,
}

/// 日志采集层
#[derive(Clone)]
pub struct LoggerLayer {
    /// 日志命令发送端
    sender: Arc<dyn LogSink>,

    /// 当前宿主项目标识
    project: String,

    /// 当前机器名称
    machine_name: String,
}

impl<S> Layer<S> for LoggerLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    /// 记录 span 创建时的字段，后续事件日志会把当前 span 链作为 scope 输出
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };

        let mut visitor = JsonVisitor::default();
        attrs.record(&mut visitor);

        span.extensions_mut().insert(SpanLogFields {
            name: span.name().to_string(),
            fields: visitor.into_fields(),
        });
    }

    /// 更新 span 字段
    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };

        let mut visitor = JsonVisitor::default();
        values.record(&mut visitor);
        let updated_fields = visitor.into_fields();

        if updated_fields.is_empty() {
            return;
        }

        let mut extensions = span.extensions_mut();
        if let Some(fields) = extensions.get_mut::<SpanLogFields>() {
            fields.fields.extend(updated_fields);
        } else {
            extensions.insert(SpanLogFields {
                name: span.name().to_string(),
                fields: updated_fields,
            });
        }
    }

    /// 采集 tracing 日志事件
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if !self.sender.accepts_target(metadata.target()) {
            return;
        }
        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);
        visitor.insert_scopes(collect_event_scopes(event, ctx));

        let pending_log = PendingLog {
            create_time: Utc::now(),
            project: self.project.clone(),
            machine_name: self.machine_name.clone(),
            category: metadata.target().to_string(),
            level: [
                "Trace",
                "Debug",
                "Information",
                "Warning",
                "Error",
                "Critical",
            ][event_level(metadata)]
            .to_string(),
            content: visitor.into_content(),
        };

        self.sender.enqueue(pending_log);
    }
}

/// 调用现场只做快照和非阻塞接收，容量与重试由具体提供者负责。
pub trait LogSink: Send + Sync {
    fn enqueue(&self, log: PendingLog);
    fn accepts_target(&self, _target: &str) -> bool {
        true
    }
}

/// span scope 中保存的日志字段
#[derive(Debug, Clone)]
struct SpanLogFields {
    /// span 名称
    name: String,

    /// span 字段集合
    fields: Map<String, Value>,
}

/// 收集当前事件所在的 span 链
fn collect_event_scopes<S>(event: &Event<'_>, ctx: Context<'_, S>) -> Vec<Value>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    let Some(scope) = ctx.event_scope(event) else {
        return Vec::new();
    };

    scope
        .from_root()
        .filter_map(|span| {
            let extensions = span.extensions();
            let fields = extensions.get::<SpanLogFields>()?;

            let mut scope = Map::new();
            scope.insert("name".to_string(), Value::String(fields.name.clone()));
            if !fields.fields.is_empty() {
                scope.insert("fields".to_string(), Value::Object(fields.fields.clone()));
            }

            Some(Value::Object(scope))
        })
        .collect()
}

/// tracing 字段访问器
#[derive(Default)]
pub struct JsonVisitor {
    /// 已采集的字段集合
    fields: Map<String, Value>,
}

impl JsonVisitor {
    /// 取出已采集字段
    pub fn into_fields(self) -> Map<String, Value> {
        self.fields
    }

    /// 插入当前 scope 信息
    pub fn insert_scopes(&mut self, scopes: Vec<Value>) {
        if !scopes.is_empty() {
            self.fields
                .insert("scopes".to_string(), Value::Array(scopes));
        }
    }

    /// 转换为最终日志内容
    pub fn into_content(mut self) -> String {
        if let Some(message) = self.fields.remove("message") {
            if self.fields.is_empty() {
                return value_to_text(message);
            }

            self.fields.insert("message".to_string(), message);
        }

        match serde_json::to_string(&self.fields) {
            Ok(value) => value,
            Err(_) => "{}".to_string(),
        }
    }
}

impl Visit for JsonVisitor {
    /// 记录 i64 字段
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), Value::Number(Number::from(value)));
    }

    /// 记录 u64 字段
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), Value::Number(Number::from(value)));
    }

    /// 记录布尔字段
    fn record_bool(&mut self, field: &Field, value: bool) {
        if field.name() == "__logger_critical" {
            return;
        }
        self.fields
            .insert(field.name().to_string(), Value::Bool(value));
    }

    /// 记录字符串字段
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().to_string(), Value::String(value.to_string()));
    }

    /// 记录错误字段
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.fields
            .insert(field.name().to_string(), Value::String(value.to_string()));
    }

    /// 记录调试字符串字段
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields.insert(
            field.name().to_string(),
            Value::String(format!("{value:?}")),
        );
    }
}

/// 注册调用现场采集层与提供者；全局只允许一个日志写入目标。
pub fn init_logger_subscriber_with_sink(
    settings: &LoggerSettings,
    sender: Arc<dyn LogSink>,
    provider: LoggerProvider,
) -> Result<LoggerFilterHandle, String> {
    let filter_handle = LoggerFilterHandle::new(settings, provider)?;
    let machine_name = get_machine_name();
    let logger_layer = LoggerLayer {
        sender,
        project: settings.project.clone(),
        machine_name,
    };

    let subscriber = Registry::default()
        .with(
            fmt::layer()
                .event_format(console::ConsoleFormat)
                .with_filter(filter_handle.console_filter()),
        )
        .with(logger_layer.with_filter(filter_handle.storage_filter()));
    tracing::subscriber::set_global_default(subscriber).map_err(|error| error.to_string())?;

    Ok(filter_handle)
}

// Critical 通过专用宏的静态字段识别，仍使用 tracing 的 ERROR 传输通道。
fn event_level(metadata: &tracing::Metadata<'_>) -> usize {
    if *metadata.level() == tracing::Level::ERROR
        && metadata.fields().field("__logger_critical").is_some()
    {
        return 5;
    }
    match *metadata.level() {
        tracing::Level::TRACE => 0,
        tracing::Level::DEBUG => 1,
        tracing::Level::INFO => 2,
        tracing::Level::WARN => 3,
        tracing::Level::ERROR => 4,
    }
}

#[doc(hidden)]
pub use tracing as __tracing;

/// 记录独立的 Critical 级别；支持与 tracing 相同的字段和消息语法。
#[macro_export]
macro_rules! critical {
    (target: $target:expr, $($fields:tt)+) => {
        $crate::__tracing::event!(target: $target, $crate::__tracing::Level::ERROR, __logger_critical = true, $($fields)+)
    };
    ($($fields:tt)+) => {
        $crate::__tracing::event!($crate::__tracing::Level::ERROR, __logger_critical = true, $($fields)+)
    };
}

/// 获取日志目录
pub fn get_log_base_path() -> PathBuf {
    PathBuf::from("Logs")
}

/// 获取机器名
pub fn get_machine_name() -> String {
    env::var("COMPUTERNAME")
        .or_else(|_| env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

/// JSON 值转文本
pub fn value_to_text(value: Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v,
        other => serde_json::to_string(&other).unwrap_or_else(|_| "{}".to_string()),
    }
}

/// 获取默认日志组件类型
fn default_logger_component() -> LoggerComponent {
    LoggerComponent::Database
}

fn default_logger_log_level() -> HashMap<String, String> {
    HashMap::from([(String::from("default"), String::from("info"))])
}

/// 获取默认日志项目标识
fn default_logger_project() -> String {
    String::new()
}

/// 获取默认日志保存天数
fn default_logger_save_days() -> i64 {
    14
}
