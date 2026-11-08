use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum Operation {
    Write,
    Recovery,
    Shutdown,
    Cleanup,
}

static CURRENT: OnceLock<Arc<Diagnostics>> = OnceLock::new();
const OPERATIONS: [&str; 4] = ["write", "recovery", "shutdown", "cleanup"];

/// 拉取式诊断；计数在进程重启后归零，不在业务线程输出诊断日志。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticsSnapshot {
    /// Trace、Debug、Info、Warning、Error、Critical。
    pub dropped_by_level: [u64; 6],
    /// write、recovery、shutdown、cleanup。
    pub failures_by_operation: [u64; 4],
    pub buffered: u64,
    pub buffered_bytes: u64,
}

pub fn snapshot() -> Option<DiagnosticsSnapshot> {
    CURRENT.get().map(|state| state.snapshot())
}

pub(crate) struct Diagnostics {
    dropped: [AtomicU64; 6],
    failures: [AtomicU64; 4],
    last_report: [AtomicU64; 4],
    started: Instant,
    pub(crate) buffered: AtomicU64,
    pub(crate) buffered_bytes: AtomicU64,
}

impl Diagnostics {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            dropped: std::array::from_fn(|_| AtomicU64::new(0)),
            failures: std::array::from_fn(|_| AtomicU64::new(0)),
            last_report: std::array::from_fn(|_| AtomicU64::new(0)),
            started: Instant::now(),
            buffered: AtomicU64::new(0),
            buffered_bytes: AtomicU64::new(0),
        })
    }
    pub(crate) fn publish(self: &Arc<Self>) {
        if CURRENT.set(self.clone()).is_ok() {
            register_metrics();
        }
    }
    pub(crate) fn dropped(&self, level: &str) {
        self.dropped[level_index(level)].fetch_add(1, Ordering::Relaxed);
    }
    pub(crate) fn failure(&self, operation: Operation, error: impl std::fmt::Display) {
        let operation = operation as usize;
        self.failures[operation].fetch_add(1, Ordering::Relaxed);
        let now = self.started.elapsed().as_millis().min(u64::MAX as u128 - 1) as u64 + 1;
        let last = self.last_report[operation].load(Ordering::Relaxed);
        if (last == 0 || now.saturating_sub(last) >= 30_000)
            && self.last_report[operation]
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            // 不输出可能携带日志正文或连接凭据的错误内容；标准错误不可写不影响计数。
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr().lock(),
                "[logger_local_file] {}: {}; 请检查日志目录和诊断指标",
                OPERATIONS[operation],
                std::any::type_name_of_val(&error)
            );
        }
    }
    fn snapshot(&self) -> DiagnosticsSnapshot {
        DiagnosticsSnapshot {
            dropped_by_level: std::array::from_fn(|i| self.dropped[i].load(Ordering::Relaxed)),
            failures_by_operation: std::array::from_fn(|i| {
                self.failures[i].load(Ordering::Relaxed)
            }),
            buffered: self.buffered.load(Ordering::Relaxed),
            buffered_bytes: self.buffered_bytes.load(Ordering::Relaxed),
        }
    }
}

/// 只注册拉取回调；日志调用现场仍仅更新原子计数，不执行导出或监控代码。
fn register_metrics() {
    use opentelemetry::{KeyValue, global};
    let meter = global::meter("Logger.LocalFile");
    meter
        .u64_observable_counter("logger.localfile.dropped")
        .with_unit("{log}")
        .with_callback(|observer| {
            if let Some(state) = CURRENT.get() {
                for (index, level) in [
                    "Trace",
                    "Debug",
                    "Information",
                    "Warning",
                    "Error",
                    "Critical",
                ]
                .into_iter()
                .enumerate()
                {
                    observer.observe(
                        state.dropped[index].load(Ordering::Relaxed),
                        &[KeyValue::new("level", level)],
                    );
                }
                // None 不接收业务日志，源指标仍公开这一零值级别。
                observer.observe(0, &[KeyValue::new("level", "None")]);
            }
        })
        .build();
    meter
        .u64_observable_counter("logger.localfile.failures")
        .with_unit("{failure}")
        .with_callback(|observer| {
            if let Some(state) = CURRENT.get() {
                for (index, operation) in OPERATIONS.into_iter().enumerate() {
                    observer.observe(
                        state.failures[index].load(Ordering::Relaxed),
                        &[KeyValue::new("operation", operation)],
                    );
                }
            }
        })
        .build();
    meter
        .u64_observable_gauge("logger.localfile.buffered")
        .with_unit("{log}")
        .with_callback(|observer| {
            if let Some(state) = CURRENT.get() {
                observer.observe(state.buffered.load(Ordering::Relaxed), &[]);
            }
        })
        .build();
    meter
        .u64_observable_gauge("logger.localfile.buffered.bytes")
        .with_unit("By")
        .with_callback(|observer| {
            if let Some(state) = CURRENT.get() {
                observer.observe(state.buffered_bytes.load(Ordering::Relaxed), &[]);
            }
        })
        .build();
}

pub(crate) fn level_index(level: &str) -> usize {
    match level {
        "Trace" => 0,
        "Debug" => 1,
        "Warning" => 3,
        "Error" => 4,
        "Critical" => 5,
        _ => 2,
    }
}
