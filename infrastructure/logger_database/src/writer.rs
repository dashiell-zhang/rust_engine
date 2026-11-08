use crate::diagnostics::{Diagnostics, Operation, level_index};
use crate::file_format::FileLine;
use chrono::Utc;
use common::identifier_generator::IdentifierGenerator;
use logger::{AsyncLogWriter, LogBufferSettings, LogSink, PendingLog};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{io::AsyncWriteExt, sync::mpsc, time::Instant};
use tokio_util::sync::CancellationToken;

struct BufferState {
    regular_bytes: AtomicU64,
    important_bytes: AtomicU64,
    accepting: AtomicBool,
    diagnostics: Arc<Diagnostics>,
}

/// 预算跟随条目保留到落盘成功或最终舍弃，取出队列不会提前释放字节预算。
struct BufferedLog {
    log: Option<PendingLog>,
    id: Option<i64>,
    bytes: u64,
    important: bool,
    count_drop: bool,
    state: Arc<BufferState>,
}

impl Drop for BufferedLog {
    fn drop(&mut self) {
        if self.count_drop
            && let Some(log) = &self.log
        {
            self.state.diagnostics.dropped(&log.level);
        }
        let used = if self.important {
            &self.state.important_bytes
        } else {
            &self.state.regular_bytes
        };
        used.fetch_sub(self.bytes, Ordering::Relaxed);
        self.state
            .diagnostics
            .buffered
            .fetch_sub(1, Ordering::Relaxed);
        self.state
            .diagnostics
            .buffered_bytes
            .fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

pub(crate) struct DatabaseLogWriter {
    regular: mpsc::Sender<BufferedLog>,
    important: mpsc::Sender<BufferedLog>,
    state: Arc<BufferState>,
    settings: LogBufferSettings,
    closing: CancellationToken,
    abort: CancellationToken,
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl DatabaseLogWriter {
    pub(crate) fn start(
        settings: LogBufferSettings,
        ids: IdentifierGenerator,
        path: PathBuf,
        diagnostics: Arc<Diagnostics>,
    ) -> Arc<Self> {
        let (regular, regular_rx) = mpsc::channel(settings.queue_capacity);
        let (important, important_rx) = mpsc::channel(settings.important_queue_capacity);
        let state = Arc::new(BufferState {
            regular_bytes: AtomicU64::new(0),
            important_bytes: AtomicU64::new(0),
            accepting: AtomicBool::new(true),
            diagnostics,
        });
        let closing = CancellationToken::new();
        let abort = CancellationToken::new();
        let worker = Worker {
            regular: regular_rx,
            important: important_rx,
            state: state.clone(),
            settings: settings.clone(),
            closing: closing.clone(),
            abort: abort.clone(),
            ids,
            path,
        };
        let task = tokio::spawn(worker.run());
        Arc::new(Self {
            regular,
            important,
            state,
            settings,
            closing,
            abort,
            task: tokio::sync::Mutex::new(Some(task)),
        })
    }

    fn try_enqueue(&self, log: PendingLog, bytes: u64, important: bool) -> Option<PendingLog> {
        let (used, limit, sender) = if important {
            (
                &self.state.important_bytes,
                self.settings.important_max_buffered_bytes,
                &self.important,
            )
        } else {
            (
                &self.state.regular_bytes,
                self.settings.max_buffered_bytes,
                &self.regular,
            )
        };
        if used
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes).filter(|next| *next <= limit)
            })
            .is_err()
        {
            return Some(log);
        }
        self.state
            .diagnostics
            .buffered
            .fetch_add(1, Ordering::Relaxed);
        self.state
            .diagnostics
            .buffered_bytes
            .fetch_add(bytes, Ordering::Relaxed);
        let entry = BufferedLog {
            log: Some(log),
            id: None,
            bytes,
            important,
            count_drop: true,
            state: self.state.clone(),
        };
        match sender.try_send(entry) {
            Ok(()) => None,
            Err(error) => {
                let mut entry = error.into_inner();
                entry.count_drop = false;
                entry.log.take()
            }
        }
    }
}

impl LogSink for DatabaseLogWriter {
    fn accepts_target(&self, target: &str) -> bool {
        !target.starts_with("repository_core::partition") && !target.starts_with("logger_database")
    }
    fn enqueue(&self, log: PendingLog) {
        if !self.state.accepting.load(Ordering::Acquire) {
            self.state.diagnostics.dropped(&log.level);
            return;
        }
        let bytes = [
            &log.content,
            &log.category,
            &log.project,
            &log.machine_name,
            &log.level,
        ]
        .into_iter()
        .fold(512u64, |total, value| {
            total.saturating_add((value.encode_utf16().count() as u64).saturating_mul(2))
        });
        if let Some(log) = self.try_enqueue(log, bytes, false) {
            if level_index(&log.level) >= 3 {
                if let Some(log) = self.try_enqueue(log, bytes, true) {
                    self.state.diagnostics.dropped(&log.level);
                }
            } else {
                self.state.diagnostics.dropped(&log.level);
            }
        }
    }
}

impl AsyncLogWriter for DatabaseLogWriter {
    fn shutdown(
        &self,
        timeout: Duration,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.state.accepting.store(false, Ordering::Release);
            self.closing.cancel();
            let mut task_slot = self.task.lock().await;
            if let Some(task) = task_slot.as_mut() {
                match tokio::time::timeout(timeout, &mut *task).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => self.state.diagnostics.failure(Operation::Shutdown, error),
                    Err(_) => {
                        self.state
                            .diagnostics
                            .failure(Operation::Shutdown, "排空期限已耗尽");
                        self.abort.cancel();
                        task.abort();
                        let _ = task.await;
                    }
                }
            }
            task_slot.take();
        })
    }
    fn cancel(&self) {
        self.state.accepting.store(false, Ordering::Release);
        self.closing.cancel();
        self.abort.cancel();
    }
}

impl Drop for DatabaseLogWriter {
    fn drop(&mut self) {
        self.state.accepting.store(false, Ordering::Release);
        self.abort.cancel();
        if let Some(task) = self.task.get_mut().take() {
            task.abort();
        }
    }
}

struct Worker {
    regular: mpsc::Receiver<BufferedLog>,
    important: mpsc::Receiver<BufferedLog>,
    state: Arc<BufferState>,
    settings: LogBufferSettings,
    closing: CancellationToken,
    abort: CancellationToken,
    ids: IdentifierGenerator,
    path: PathBuf,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.state.accepting.store(false, Ordering::Release);
    }
}

impl Worker {
    async fn run(mut self) {
        let mut batch = Vec::with_capacity(self.settings.batch_size.min(1024));
        let mut utf8 = Vec::with_capacity(4096);
        let interval = Duration::from_millis(self.settings.flush_interval_milliseconds);
        let mut due = Instant::now() + interval;
        let mut retry_seconds = 1;
        loop {
            if self.abort.is_cancelled() {
                break;
            }
            if self.closing.is_cancelled() {
                self.regular.close();
                self.important.close();
            }
            while batch.len() < self.settings.batch_size {
                match self
                    .important
                    .try_recv()
                    .or_else(|_| self.regular.try_recv())
                {
                    Ok(entry) => batch.push(entry),
                    Err(_) => break,
                }
            }
            if self.closing.is_cancelled() && batch.is_empty() {
                break;
            }
            if !batch.is_empty()
                && (batch.len() >= self.settings.batch_size
                    || Instant::now() >= due
                    || self.closing.is_cancelled()
                    || retry_seconds > 1)
            {
                let result = tokio::select! {
                    biased;
                    _ = self.abort.cancelled() => break,
                    result = flush_batch(&self.path, &self.ids, &mut batch, &mut utf8) => result,
                };
                if utf8.capacity() > 65536 {
                    utf8 = Vec::with_capacity(4096);
                } else {
                    utf8.clear();
                }
                match result {
                    Ok(()) => {
                        for entry in &mut batch {
                            entry.count_drop = false;
                        }
                        batch.clear();
                        retry_seconds = 1;
                        due = Instant::now() + interval;
                    }
                    Err(error) => {
                        self.state.diagnostics.failure(Operation::Spool, error);
                        tokio::select! { _ = self.abort.cancelled() => break,
                        _ = tokio::time::sleep(Duration::from_secs(retry_seconds)) => {} }
                        retry_seconds = (retry_seconds * 2).min(30);
                    }
                }
                continue;
            }
            tokio::select! {
                biased;
                _ = self.abort.cancelled() => break,
                _ = self.closing.cancelled(), if !self.regular.is_closed() => {},
                Some(entry) = self.important.recv() => batch.push(entry),
                Some(entry) = self.regular.recv() => batch.push(entry),
                _ = tokio::time::sleep_until(due) => { if batch.is_empty() { due = Instant::now() + interval; } },
            }
        }
        // 未落盘批次和队列随作用域释放，由预算守卫统一计入舍弃并归还字节数。
    }
}

async fn flush_batch(
    path: &Path,
    ids: &IdentifierGenerator,
    batch: &mut [BufferedLog],
    utf8: &mut Vec<u8>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tokio::fs::create_dir_all(path).await?;
    let name = format!(
        "batch-{}-{}.log",
        Utc::now().format("%Y%m%d%H%M%S%3f"),
        uuid::Uuid::new_v4().simple()
    );
    let temporary = path.join(format!("{name}.tmp"));
    let final_path = path.join(name);
    let file = tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary)?;
        file.try_lock().map_err(std::io::Error::other)?;
        Ok(file)
    })
    .await??;
    let mut writer = tokio::io::BufWriter::with_capacity(
        file_buffer_size(batch),
        tokio::fs::File::from_std(file),
    );
    for entry in batch {
        if entry.id.is_none() {
            entry.id = Some(ids.get_id().await?);
        }
        let log = entry.log.as_ref().expect("缓冲中的日志");
        let line = FileLine {
            id: entry.id.expect("已生成 ID"),
            project: &log.project,
            machine_name: &log.machine_name,
            level: &log.level,
            category: &log.category,
            content: &log.content,
            create_time: log.create_time,
            delete_time: None,
            row_version: 0,
        };
        utf8.clear();
        serde_json::to_writer(&mut *utf8, &line)?;
        utf8.push(b'\n');
        writer.write_all(utf8).await?;
    }
    writer.flush().await?;
    let file = writer.into_inner();
    drop(file.into_std().await);
    // 任何失败留下临时文件。跨重试的条目保留同一 ID，恢复和入库负责去重。
    crate::spool::publish_file(&final_path.with_extension("log.tmp"), &final_path).await?;
    Ok(())
}

fn file_buffer_size(batch: &[BufferedLog]) -> usize {
    let mut estimate = 0usize;
    for entry in batch {
        let log = entry.log.as_ref().expect("缓冲中的日志");
        estimate = estimate.saturating_add(384);
        for value in [
            &log.project,
            &log.machine_name,
            &log.level,
            &log.category,
            &log.content,
        ] {
            if estimate.saturating_add(value.len()) > 16384 {
                return 65536;
            }
            for ch in value.chars() {
                estimate += match ch {
                    '"' | '\\' => 2,
                    '\0'..='\u{1f}' => 6,
                    _ => ch.len_utf8(),
                };
                if estimate > 16384 {
                    return 65536;
                }
            }
        }
    }
    16384
}
