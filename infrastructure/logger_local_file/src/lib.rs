use std::{
    collections::BTreeSet,
    fs::{self as std_fs, File as StdFile},
    io::{BufWriter, Write},
    path::Path,
    sync::mpsc::{Receiver, RecvTimeoutError},
    thread,
    time::Duration as StdDuration,
};

use chrono::{DateTime, Utc};
use logger::{
    FLUSH_INTERVAL_SECONDS, LogCommand, LoggerRuntime, LoggerSettings, MAX_BATCH_SIZE, PendingLog,
    get_log_base_path, init_logger_subscriber,
};
use serde_json::json;
use tokio::{
    fs,
    time::{self, Duration},
};

/// 本地文件日志项
#[derive(Debug, Clone)]
struct LocalFileLogEntry {
    /// 日志所属小时键
    hour_key: String,

    /// 日志文本内容
    line: String,
}

/// 本地文件日志分组
struct LocalFileLogGroup {
    /// 日志所属小时键
    hour_key: String,

    /// 当前小时下的日志集合
    entries: Vec<LocalFileLogEntry>,
}

/// 初始化本地文件日志组件
///
/// 设置 tracing 全局 subscriber，启动本地文件写盘线程与清理任务
/// 返回 `LoggerRuntime` 守卫，drop 时自动刷盘并停止后台任务
pub fn init_local_file_logger(
    settings: &LoggerSettings,
) -> Result<LoggerRuntime, tracing::subscriber::SetGlobalDefaultError> {
    let (sender, receiver) = init_logger_subscriber(settings)?;

    let base_path = get_log_base_path();
    if let Err(error) = std_fs::create_dir_all(&base_path) {
        eprintln!("创建本地日志目录失败: {error}");
    }

    let writer_handle = thread::Builder::new()
        .name("local-file-log-writer".to_string())
        .spawn(move || run_local_file_log_writer(receiver))
        .expect("本地文件日志写盘线程启动失败");

    let clear_handle = tokio::spawn(run_local_file_log_clear_task(settings.save_days));

    Ok(LoggerRuntime::new(
        sender,
        writer_handle,
        None,
        Some(clear_handle),
    ))
}

/// 本地文件日志写盘线程
fn run_local_file_log_writer(receiver: Receiver<LogCommand>) {
    let base_path = get_log_base_path();
    let mut buffer = Vec::with_capacity(MAX_BATCH_SIZE);
    let flush_interval = StdDuration::from_secs(FLUSH_INTERVAL_SECONDS);

    loop {
        match receiver.recv_timeout(flush_interval) {
            Ok(LogCommand::Write(pending_log)) => {
                buffer.push(build_local_file_log_line(pending_log));

                if buffer.len() >= MAX_BATCH_SIZE {
                    flush_local_file_logs(&base_path, &mut buffer);
                }
            }
            Ok(LogCommand::Shutdown) => break,
            Err(RecvTimeoutError::Timeout) => {
                flush_local_file_logs(&base_path, &mut buffer);
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    while let Ok(LogCommand::Write(pending_log)) = receiver.try_recv() {
        if buffer.len() < MAX_BATCH_SIZE {
            buffer.push(build_local_file_log_line(pending_log));
        } else {
            flush_local_file_logs(&base_path, &mut buffer);
            buffer.push(build_local_file_log_line(pending_log));
        }
    }

    flush_local_file_logs(&base_path, &mut buffer);
}

/// 本地文件日志清理任务
async fn run_local_file_log_clear_task(save_days: i64) {
    if save_days == -1 {
        return;
    }

    loop {
        let base_path = get_log_base_path();
        let delete_time = Utc::now() - chrono::Duration::days(save_days);

        if let Err(error) = clear_local_file_logs(&base_path, delete_time).await {
            eprintln!("清理本地文件日志失败: {error}");
        }

        time::sleep(Duration::from_secs(60 * 60 * 24)).await;
    }
}

/// 批量写入本地日志文件
fn flush_local_file_logs(base_path: &Path, buffer: &mut Vec<LocalFileLogEntry>) {
    if buffer.is_empty() {
        return;
    }

    let flush_result = (|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for group in group_local_file_logs(buffer) {
            let log_path = base_path.join(format!("{}.log", group.hour_key));
            let file = StdFile::options()
                .create(true)
                .append(true)
                .open(log_path)?;
            let mut writer = BufWriter::new(file);

            for entry in group.entries {
                writer.write_all(entry.line.as_bytes())?;
                writer.write_all(b"\n")?;
            }

            writer.flush()?;
        }

        Ok(())
    })();

    if let Err(error) = flush_result {
        eprintln!("落盘本地日志文件失败: {error}");
        buffer.clear();
        return;
    }

    buffer.clear();
}

/// 清理本地文件日志
async fn clear_local_file_logs(
    base_path: &Path,
    delete_time: DateTime<Utc>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !fs::try_exists(base_path).await? {
        return Ok(());
    }

    let mut entries = fs::read_dir(base_path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if !is_local_file_log_file(&path) {
            continue;
        }

        let metadata = fs::metadata(&path).await?;
        let created_time: DateTime<Utc> =
            metadata.created().or_else(|_| metadata.modified())?.into();

        if created_time < delete_time {
            let _ = fs::remove_file(path).await;
        }
    }

    Ok(())
}

/// 组装本地文件日志结构
fn build_local_file_log_line(pending_log: PendingLog) -> LocalFileLogEntry {
    let line = json!({
        "Category": pending_log.category,
        "Level": pending_log.level,
        "Content": pending_log.content
    })
    .to_string();

    LocalFileLogEntry {
        hour_key: pending_log.create_time.format("%Y%m%d%H").to_string(),
        line,
    }
}

/// 按小时分组本地文件日志
fn group_local_file_logs(buffer: &[LocalFileLogEntry]) -> Vec<LocalFileLogGroup> {
    let mut groups = BTreeSet::new();
    for item in buffer {
        groups.insert(item.hour_key.clone());
    }

    groups
        .into_iter()
        .map(|hour_key| LocalFileLogGroup {
            entries: buffer
                .iter()
                .filter(|item| item.hour_key == hour_key)
                .cloned()
                .collect(),
            hour_key,
        })
        .collect()
}

/// 判断是否为本地文件日志文件
fn is_local_file_log_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    file_name.len() == 14
        && file_name.ends_with(".log")
        && file_name[..10].chars().all(|ch| ch.is_ascii_digit())
}
