use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::JoinHandle,
};

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
    Layer,
    filter::{LevelFilter, Targets},
    fmt,
    layer::Context,
    layer::SubscriberExt,
    registry::{LookupSpan, Registry},
};

/// 日志通道容量
pub const MAX_BATCH_SIZE: usize = 1000;

/// 日志刷盘间隔（秒）
pub const FLUSH_INTERVAL_SECONDS: u64 = 3;

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
    /// 日志组件类型
    #[serde(default = "default_logger_component")]
    pub component: LoggerComponent,

    /// 日志级别配置，key 为 tracing target，default 表示默认级别
    #[serde(default = "default_logger_log_level")]
    pub log_level: HashMap<String, String>,

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
            component: default_logger_component(),
            log_level: default_logger_log_level(),
            project: default_logger_project(),
            save_days: default_logger_save_days(),
        }
    }
}

/// 日志运行时守卫
///
/// 持有写盘线程与可选后台任务
/// 进程退出时负责刷新剩余日志
pub struct LoggerRuntime {
    /// 日志写盘通道发送端
    writer_sender: Option<SyncSender<LogCommand>>,

    /// 日志写盘线程句柄
    writer_handle: Option<JoinHandle<()>>,

    /// 数据库日志入库任务句柄（仅 Database 组件使用）
    save_handle: Option<TokioJoinHandle<()>>,

    /// 日志清理任务句柄
    clear_handle: Option<TokioJoinHandle<()>>,
}

impl LoggerRuntime {
    /// 由提供者 crate 构造运行时守卫
    pub fn new(
        sender: SyncSender<LogCommand>,
        writer_handle: JoinHandle<()>,
        save_handle: Option<TokioJoinHandle<()>>,
        clear_handle: Option<TokioJoinHandle<()>>,
    ) -> Self {
        Self {
            writer_sender: Some(sender),
            writer_handle: Some(writer_handle),
            save_handle,
            clear_handle,
        }
    }
}

impl Drop for LoggerRuntime {
    /// 释放日志运行时资源
    fn drop(&mut self) {
        if let Some(sender) = self.writer_sender.take() {
            let _ = sender.send(LogCommand::Shutdown);
        }

        if let Some(handle) = self.writer_handle.take() {
            let _ = handle.join();
        }

        if let Some(handle) = self.save_handle.take() {
            handle.abort();
        }

        if let Some(handle) = self.clear_handle.take() {
            handle.abort();
        }
    }
}

/// 日志写盘命令
pub enum LogCommand {
    /// 写入单条待处理日志
    Write(PendingLog),

    /// 关闭日志写盘线程
    Shutdown,
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
    sender: SyncSender<LogCommand>,

    /// 因队列已满而丢弃的日志条数
    dropped_logs: Arc<AtomicU64>,

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
        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);
        visitor.insert_scopes(collect_event_scopes(event, ctx));

        let pending_log = PendingLog {
            create_time: Utc::now(),
            project: self.project.clone(),
            machine_name: self.machine_name.clone(),
            category: metadata.target().to_string(),
            level: metadata.level().to_string(),
            content: visitor.into_content(),
        };

        match self.sender.try_send(LogCommand::Write(pending_log)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                let dropped = self.dropped_logs.fetch_add(1, Ordering::Relaxed) + 1;

                if dropped == 1 || dropped.is_multiple_of(100) {
                    eprintln!("日志队列已满，已丢弃 {dropped} 条日志");
                }
            }
            Err(TrySendError::Disconnected(_)) => {
                eprintln!("日志写入线程已停止，当前日志写入失败");
            }
        }
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

/// 初始化 tracing subscriber 并返回日志通道
///
/// 创建日志采集层与 mpsc 通道，将 `LoggerLayer` 注册为全局 tracing subscriber
/// 返回的 sender 已注入到 `LoggerLayer` 中，receiver 交由提供者 crate 启动写盘线程
pub fn init_logger_subscriber(
    settings: &LoggerSettings,
) -> Result<
    (SyncSender<LogCommand>, Receiver<LogCommand>),
    tracing::subscriber::SetGlobalDefaultError,
> {
    let machine_name = get_machine_name();
    let (sender, receiver) = mpsc::sync_channel::<LogCommand>(MAX_BATCH_SIZE);

    let logger_layer = LoggerLayer {
        sender: sender.clone(),
        dropped_logs: Arc::new(AtomicU64::new(0)),
        project: settings.project.clone(),
        machine_name,
    };

    let subscriber = Registry::default()
        .with(build_targets_filter(settings))
        .with(fmt::layer())
        .with(logger_layer);
    tracing::subscriber::set_global_default(subscriber)?;

    Ok((sender, receiver))
}

fn build_targets_filter(settings: &LoggerSettings) -> Targets {
    let mut default_level = LevelFilter::INFO;
    let mut targets = Vec::new();

    for (target, level_text) in &settings.log_level {
        let normalized_target = target.trim();

        if normalized_target.is_empty() {
            continue;
        }

        let Some(level) = parse_level_filter(level_text) else {
            eprintln!("日志级别配置无效，已忽略: target={normalized_target}, level={level_text}");
            continue;
        };

        if normalized_target.eq_ignore_ascii_case("default") {
            default_level = level;
        } else {
            targets.push((normalized_target.to_string(), level));
        }
    }

    let mut filter = Targets::new().with_default(default_level);

    for (target, level) in targets {
        filter = filter.with_target(target, level);
    }

    filter
}

fn parse_level_filter(level_text: &str) -> Option<LevelFilter> {
    match level_text.trim().to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::OFF),
        "error" => Some(LevelFilter::ERROR),
        "warn" | "warning" => Some(LevelFilter::WARN),
        "info" | "information" => Some(LevelFilter::INFO),
        "debug" => Some(LevelFilter::DEBUG),
        "trace" => Some(LevelFilter::TRACE),
        _ => None,
    }
}

/// 获取日志目录
pub fn get_log_base_path() -> PathBuf {
    PathBuf::from("logs")
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
