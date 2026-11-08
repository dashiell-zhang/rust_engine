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
    Spool,
    Database,
    Recovery,
    Shutdown,
    Quarantine,
    Cleanup,
}

static CURRENT: OnceLock<Arc<Diagnostics>> = OnceLock::new();
const OPERATIONS: [&str; 6] = [
    "spool",
    "database",
    "recovery",
    "shutdown",
    "quarantine",
    "cleanup",
];

/// 拉取式诊断；计数在进程重启后归零，不在业务线程输出诊断日志。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticsSnapshot {
    /// Trace、Debug、Info、Warning、Error、Critical。
    pub dropped_by_level: [u64; 6],
    /// spool、database、recovery、shutdown、quarantine、cleanup。
    pub failures_by_operation: [u64; 6],
    pub buffered: u64,
    pub buffered_bytes: u64,
    pub spool_files: u64,
    pub spool_bytes: u64,
    pub rejected_files: u64,
}

pub fn snapshot() -> Option<DiagnosticsSnapshot> {
    CURRENT.get().map(|state| state.snapshot())
}

pub(crate) struct Diagnostics {
    dropped: [AtomicU64; 6],
    failures: [AtomicU64; 6],
    last_report: [AtomicU64; 6],
    started: Instant,
    pub(crate) buffered: AtomicU64,
    pub(crate) buffered_bytes: AtomicU64,
    pub(crate) spool_files: AtomicU64,
    pub(crate) spool_bytes: AtomicU64,
    pub(crate) rejected_files: AtomicU64,
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
            spool_files: AtomicU64::new(0),
            spool_bytes: AtomicU64::new(0),
            rejected_files: AtomicU64::new(0),
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
                "[logger_database] {}: {}; 请检查日志目录、数据库和诊断指标",
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
            spool_files: self.spool_files.load(Ordering::Relaxed),
            spool_bytes: self.spool_bytes.load(Ordering::Relaxed),
            rejected_files: self.rejected_files.load(Ordering::Relaxed),
        }
    }
}

/// 标准监控回调只读取既有原子快照，不扫描磁盘、不查询数据库，也不回写日志。
fn register_metrics() {
    use opentelemetry::{KeyValue, global};
    let meter = global::meter("Logger.Database");
    meter
        .u64_observable_counter("logger.database.dropped")
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
                observer.observe(0, &[KeyValue::new("level", "None")]);
            }
        })
        .build();
    meter
        .u64_observable_counter("logger.database.failures")
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
    for (name, unit, read) in [
        (
            "logger.database.buffered",
            "{log}",
            (|state: &Diagnostics| &state.buffered) as fn(&Diagnostics) -> &AtomicU64,
        ),
        (
            "logger.database.buffered.bytes",
            "By",
            |state: &Diagnostics| &state.buffered_bytes,
        ),
        (
            "logger.database.spool.files",
            "{file}",
            |state: &Diagnostics| &state.spool_files,
        ),
        (
            "logger.database.spool.bytes",
            "By",
            |state: &Diagnostics| &state.spool_bytes,
        ),
        (
            "logger.database.rejected.files",
            "{file}",
            |state: &Diagnostics| &state.rejected_files,
        ),
    ] {
        meter
            .u64_observable_gauge(name)
            .with_unit(unit)
            .with_callback(move |observer| {
                if let Some(state) = CURRENT.get() {
                    observer.observe(read(state).load(Ordering::Relaxed), &[]);
                }
            })
            .build();
    }
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
