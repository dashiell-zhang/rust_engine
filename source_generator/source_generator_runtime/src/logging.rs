use crate::*;

/// 一次方法调用的日志生命周期；未正常结束的调用不会记录为成功。
#[doc(hidden)]
pub struct InvocationLog {
    method: String,
    trace_id: String,
    start: std::time::Instant,
    completed: bool,
    arguments: serde_json::Map<String, serde_json::Value>,
    info_enabled: bool,
    error_enabled: bool,
}

impl InvocationLog {
    pub fn new(method: String) -> Self {
        Self {
            method,
            trace_id: new_trace_id(),
            start: std::time::Instant::now(),
            completed: false,
            arguments: serde_json::Map::new(),
            info_enabled: tracing::enabled!(tracing::Level::INFO),
            error_enabled: tracing::enabled!(tracing::Level::ERROR),
        }
    }

    pub fn set_argument(&mut self, name: &str, capture: impl FnOnce() -> serde_json::Value) {
        if self.info_enabled || self.error_enabled {
            self.arguments.insert(name.into(), capture());
        }
    }

    pub fn start(&self) {
        if self.info_enabled {
            self.emit("executing", None, None);
        }
    }

    pub fn capture_stream_items(&self) -> bool {
        self.info_enabled
    }

    pub fn capture_stream_error(&self) -> bool {
        self.error_enabled
    }

    pub fn complete(&mut self) {
        self.completed = true;
        if self.info_enabled {
            self.emit("executed", None, None);
        }
    }

    pub fn complete_value<T: ?Sized>(
        &mut self,
        _value: &T,
        capture: impl FnOnce() -> serde_json::Value,
    ) {
        self.completed = true;
        if self.info_enabled {
            let result = (std::any::type_name::<T>() != "()").then(capture);
            self.emit("executed", result, None);
        }
    }

    pub fn fail(&mut self, capture: impl FnOnce() -> serde_json::Value) {
        self.completed = true;
        if self.error_enabled {
            self.emit("exception", None, Some(capture()));
        }
    }

    pub fn panic(&mut self, payload: &(dyn std::any::Any + Send)) {
        self.fail(|| crate::logging_snapshot::panic_payload(payload));
    }

    fn emit(
        &self,
        event: &str,
        result: Option<serde_json::Value>,
        exception: Option<serde_json::Value>,
    ) {
        let mut payload = serde_json::json!({
            "event": event,
            "method": self.method,
            "traceId": self.trace_id,
        });
        if !self.arguments.is_empty() {
            payload["args"] = serde_json::Value::Object(self.arguments.clone());
        }
        if event != "executing" {
            payload["durationMs"] = u64::try_from(self.start.elapsed().as_millis())
                .unwrap_or(u64::MAX)
                .into();
        }
        if let Some(result) = result {
            payload["result"] = result;
        }
        if let Some(exception) = exception {
            payload["exception"] = exception;
            tracing::error!("{payload}");
        } else {
            tracing::info!("{payload}");
        }
    }
}

impl Drop for InvocationLog {
    fn drop(&mut self) {
        if !self.completed {
            self.fail(|| {
                crate::logging_snapshot::exception_payload(
                    "invocation",
                    if std::thread::panicking() {
                        "方法发生 panic"
                    } else {
                        "方法调用未完成即被取消"
                    }
                    .into(),
                    None,
                )
            });
        }
    }
}

/// 生成新的调用追踪 ID
///
/// 每次方法调用时生成唯一标识，用于在日志中关联同一次调用的 executing/executed/exception 三条记录
pub fn new_trace_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// 记录方法调用开始日志
///
/// 在方法体执行前输出，包含追踪 ID 与方法路径，event 固定为 executing
pub fn log_executing(method: &str, trace_id: &str) {
    tracing::info!(trace_id, method, event = "executing",);
}

/// 记录方法调用完成日志
///
/// 在方法体正常返回后输出，包含追踪 ID、方法路径与执行耗时，event 固定为 executed
pub fn log_executed(method: &str, trace_id: &str, duration_ms: u128) {
    let duration_ms = u64::try_from(duration_ms).unwrap_or(u64::MAX);
    tracing::info!(trace_id, method, duration_ms, event = "executed",);
}

/// 记录方法调用异常日志
///
/// 在方法体返回 Err 时输出，包含追踪 ID、方法路径、执行耗时与错误描述，event 固定为 exception
pub fn log_exception(method: &str, trace_id: &str, duration_ms: u128, error: &str) {
    let duration_ms = u64::try_from(duration_ms).unwrap_or(u64::MAX);
    tracing::error!(trace_id, method, duration_ms, error, event = "exception",);
}

/// 在异常日志启用时格式化并记录方法调用错误
pub fn log_exception_debug(method: &str, trace_id: &str, duration_ms: u128, error: &impl Debug) {
    let duration_ms = u64::try_from(duration_ms).unwrap_or(u64::MAX);
    tracing::error!(
        trace_id,
        method,
        duration_ms,
        error = %LazyDebugSummary(error),
        event = "exception",
    );
}

/// 生成适合写入日志的有界错误摘要
pub fn error_summary(error: &impl std::fmt::Debug) -> String {
    format_error_summary(format_args!("{error:?}"))
}

/// 生成适合写入重试日志的有界错误摘要
pub fn retry_error_summary(error: &impl std::fmt::Debug) -> String {
    error_summary(error)
}

/// 生成适合写入日志的有界展示文本
pub(crate) fn display_summary(value: &dyn Display) -> String {
    format_error_summary(format_args!("{value}"))
}

/// 将格式化参数写入有界且无控制字符的摘要
pub(crate) fn format_error_summary(arguments: fmt::Arguments<'_>) -> String {
    const MAX_CHARS: usize = 512;

    let mut writer = ErrorSummaryWriter::new(MAX_CHARS);
    let _ = fmt::write(&mut writer, arguments);
    writer.finish()
}

/// 在格式化阶段限制错误摘要的字符数量
pub(crate) struct ErrorSummaryWriter {
    summary: String,
    max_chars: usize,
    written_chars: usize,
    truncated: bool,
}

impl ErrorSummaryWriter {
    /// 创建有界错误摘要写入器
    fn new(max_chars: usize) -> Self {
        Self {
            summary: String::with_capacity(max_chars),
            max_chars,
            written_chars: 0,
            truncated: false,
        }
    }

    /// 完成摘要并在截断时追加标记
    fn finish(mut self) -> String {
        if self.truncated {
            self.summary.push('…');
        }

        self.summary
    }
}

impl fmt::Write for ErrorSummaryWriter {
    /// 写入不超过字符上限的错误文本并清理控制字符
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for character in value.chars() {
            if self.written_chars >= self.max_chars {
                self.truncated = true;
                return Err(fmt::Error);
            }

            self.written_chars += 1;
            if character.is_control() {
                if !self.summary.ends_with(' ') {
                    self.summary.push(' ');
                }
            } else {
                self.summary.push(character);
            }
        }

        Ok(())
    }
}

/// 在最终 tracing 事件实际记录时才格式化 Debug 错误摘要
pub(crate) struct LazyDebugSummary<'a, T: ?Sized>(&'a T);

impl<T> Display for LazyDebugSummary<'_, T>
where
    T: Debug + ?Sized,
{
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&format_error_summary(format_args!("{:?}", self.0)))
    }
}

/// 在最终 tracing 事件实际记录时才格式化 Display 错误摘要
pub(crate) struct LazyDisplaySummary<'a, T: ?Sized>(&'a T);

impl<T> Display for LazyDisplaySummary<'_, T>
where
    T: Display + ?Sized,
{
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&format_error_summary(format_args!("{}", self.0)))
    }
}

/// 记录即将执行下一次重试
pub fn log_retry_scheduled(
    method: &str,
    trace_id: &str,
    retry: u64,
    delay_ms: u64,
    error_type: &str,
    error: &str,
) {
    tracing::warn!(
        method,
        trace_id,
        retry,
        delay_ms,
        error_type,
        error,
        event = "retry_scheduled"
    );
}

/// 在警告日志启用时格式化并记录即将执行的下一次重试
pub fn log_retry_scheduled_debug(
    method: &str,
    trace_id: &str,
    retry: u64,
    delay_ms: u64,
    error: &impl Debug,
) {
    let error_type = std::any::type_name_of_val(error);
    tracing::warn!(
        method,
        trace_id,
        retry,
        delay_ms,
        error_type,
        error = %LazyDebugSummary(error),
        event = "retry_scheduled"
    );
}

/// 记录重试次数已经耗尽
pub fn log_retry_exhausted(
    method: &str,
    trace_id: &str,
    retries: u64,
    error_type: &str,
    error: &str,
) {
    tracing::error!(
        method,
        trace_id,
        retries,
        error_type,
        error,
        event = "retry_exhausted"
    );
}

/// 在错误日志启用时格式化并记录重试次数耗尽错误
pub fn log_retry_exhausted_debug(method: &str, trace_id: &str, retries: u64, error: &impl Debug) {
    let error_type = std::any::type_name_of_val(error);
    tracing::error!(
        method,
        trace_id,
        retries,
        error_type,
        error = %LazyDebugSummary(error),
        event = "retry_exhausted"
    );
}

/// 记录缓存读取、写入或防击穿锁故障并继续执行业务
pub fn log_cache_degraded(method: &str, operation: &str, error: &dyn Display) {
    tracing::warn!(
        method,
        operation,
        error = %LazyDisplaySummary(error),
        event = "cache_degraded"
    );
}
