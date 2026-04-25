use std::{
    collections::BTreeSet,
    fs::{self as std_fs, File as StdFile},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{Receiver, RecvTimeoutError},
    },
    thread,
    time::Duration as StdDuration,
};

use chrono::{DateTime, Utc};
use common::identifier_generator::IdentifierGenerator;
use logger::{
    FLUSH_INTERVAL_SECONDS, LogCommand, LoggerRuntime, LoggerSettings, MAX_BATCH_SIZE, PendingLog,
    get_log_base_path, init_logger_subscriber,
};
use repository::log;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QuerySelect,
};
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::{AsyncBufReadExt, BufReader},
    time::{self, Duration},
};

const LOG_FILE_SCAN_LIMIT: usize = 50;
const SAVE_INTERVAL_SECONDS: u64 = 5;
const TMP_RECOVER_AGE_SECONDS: i64 = 300;

/// 数据库日志文件中单行的结构
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DatabaseFileLogLine {
    /// 日志主键
    id: i64,

    /// 项目标识
    project: String,

    /// 机器名称
    machine_name: String,

    /// 日志级别
    level: String,

    /// 日志分类
    category: String,

    /// 日志内容
    content: String,

    /// 日志创建时间
    create_time: DateTime<Utc>,
}

/// 初始化数据库日志组件
///
/// 设置 tracing 全局 subscriber，启动数据库写盘线程、入库任务与清理任务
/// 返回 `LoggerRuntime` 守卫，drop 时自动刷盘并停止后台任务
pub fn init_database_logger(
    settings: &LoggerSettings,
    db_pool: Arc<DatabaseConnection>,
    identifier_generator: IdentifierGenerator,
) -> Result<LoggerRuntime, tracing::subscriber::SetGlobalDefaultError> {
    let (sender, receiver) = init_logger_subscriber(settings)?;

    let base_path = get_log_base_path();
    if let Err(error) = std_fs::create_dir_all(&base_path) {
        eprintln!("创建数据库日志目录失败: {error}");
    }

    let writer_identifier_generator = identifier_generator.clone();
    let writer_handle = thread::Builder::new()
        .name("database-log-writer".to_string())
        .spawn(move || run_database_log_writer(receiver, writer_identifier_generator))
        .expect("数据库日志写盘线程启动失败");

    let save_handle = tokio::spawn(run_log_save_task(Arc::clone(&db_pool)));
    let clear_handle = tokio::spawn(run_database_log_clear_task(
        Arc::clone(&db_pool),
        settings.save_days,
    ));

    Ok(LoggerRuntime::new(
        sender,
        writer_handle,
        Some(save_handle),
        Some(clear_handle),
    ))
}

/// 数据库日志写盘线程
fn run_database_log_writer(
    receiver: Receiver<LogCommand>,
    identifier_generator: IdentifierGenerator,
) {
    let base_path = get_log_base_path();
    let mut buffer = Vec::with_capacity(MAX_BATCH_SIZE);
    let flush_interval = StdDuration::from_secs(FLUSH_INTERVAL_SECONDS);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("数据库日志运行时创建失败");

    loop {
        match receiver.recv_timeout(flush_interval) {
            Ok(LogCommand::Write(pending_log)) => {
                if let Some(file_log) =
                    build_database_file_log_line(&runtime, &identifier_generator, pending_log)
                {
                    buffer.push(file_log);

                    if buffer.len() >= MAX_BATCH_SIZE {
                        flush_database_logs(&base_path, &mut buffer);
                    }
                }
            }
            Ok(LogCommand::Shutdown) => break,
            Err(RecvTimeoutError::Timeout) => {
                flush_database_logs(&base_path, &mut buffer);
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    while let Ok(LogCommand::Write(pending_log)) = receiver.try_recv() {
        if let Some(file_log) =
            build_database_file_log_line(&runtime, &identifier_generator, pending_log)
        {
            if buffer.len() < MAX_BATCH_SIZE {
                buffer.push(file_log);
            } else {
                flush_database_logs(&base_path, &mut buffer);
                buffer.push(file_log);
            }
        }
    }

    flush_database_logs(&base_path, &mut buffer);
}

/// 数据库日志入库任务
async fn run_log_save_task(db_pool: Arc<DatabaseConnection>) {
    loop {
        let base_path = get_log_base_path();

        if let Err(error) = recover_tmp_files(&base_path).await {
            eprintln!("恢复数据库临时日志文件失败: {error}");
        }

        if let Err(error) = save_logs_to_database(&db_pool, &base_path).await {
            eprintln!("数据库日志入库失败: {error}");
        }

        time::sleep(Duration::from_secs(SAVE_INTERVAL_SECONDS)).await;
    }
}

/// 数据库日志清理任务
async fn run_database_log_clear_task(db_pool: Arc<DatabaseConnection>, save_days: i64) {
    if save_days == -1 {
        return;
    }

    loop {
        let delete_time = Utc::now() - chrono::Duration::days(save_days);

        if let Err(error) = log::Entity::delete_many()
            .filter(log::Column::CreateTime.lte(delete_time))
            .exec(&*db_pool)
            .await
        {
            eprintln!("清理数据库日志失败: {error}");
        }

        time::sleep(Duration::from_secs(60 * 60 * 24)).await;
    }
}

/// 批量写入数据库日志文件
fn flush_database_logs(base_path: &Path, buffer: &mut Vec<DatabaseFileLogLine>) {
    if buffer.is_empty() {
        return;
    }

    let file_name = format!(
        "batch-{}-{}.log",
        Utc::now().format("%Y%m%d%H%M%S%3f"),
        uuid_like_suffix()
    );
    let temp_path = base_path.join(format!("{file_name}.tmp"));
    let final_path = base_path.join(file_name);

    let flush_result = (|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let file = StdFile::create(&temp_path)?;
        let mut writer = BufWriter::new(file);

        for line in buffer.iter() {
            writer.write_all(serde_json::to_string(line)?.as_bytes())?;
            writer.write_all(b"\n")?;
        }

        writer.flush()?;
        drop(writer);

        std_fs::rename(&temp_path, &final_path)?;
        Ok(())
    })();

    if let Err(error) = flush_result {
        eprintln!("落盘数据库日志文件失败: {error}");
        let _ = std_fs::remove_file(&temp_path);
        buffer.clear();
        return;
    }

    buffer.clear();
}

/// 扫描数据库日志文件并写入数据库
async fn save_logs_to_database(
    db_pool: &DatabaseConnection,
    base_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !fs::try_exists(base_path).await? {
        return Ok(());
    }

    let mut file_paths = Vec::new();
    let mut entries = fs::read_dir(base_path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if !is_database_batch_log_file(&path) {
            continue;
        }

        file_paths.push(path);
    }

    file_paths.sort();

    for log_path in file_paths.into_iter().take(LOG_FILE_SCAN_LIMIT) {
        let logs = read_database_log_lines(&log_path).await?;

        if !logs.is_empty() {
            insert_log_lines(db_pool, logs).await?;
        }

        fs::remove_file(&log_path).await?;
    }

    Ok(())
}

/// 读取单个数据库日志文件
async fn read_database_log_lines(
    path: &Path,
) -> Result<Vec<DatabaseFileLogLine>, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut logs = Vec::new();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(log_line) = serde_json::from_str::<DatabaseFileLogLine>(&line) {
            logs.push(log_line);
        }
    }

    Ok(logs)
}

/// 组装数据库日志结构
fn build_database_file_log_line(
    runtime: &tokio::runtime::Runtime,
    identifier_generator: &IdentifierGenerator,
    pending_log: PendingLog,
) -> Option<DatabaseFileLogLine> {
    let id = match runtime.block_on(identifier_generator.get_id()) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("数据库日志生成 ID 失败: {error}");
            return None;
        }
    };

    Some(DatabaseFileLogLine {
        id,
        project: pending_log.project,
        machine_name: pending_log.machine_name,
        level: pending_log.level,
        category: pending_log.category,
        content: pending_log.content,
        create_time: pending_log.create_time,
    })
}

/// 批量插入数据库日志
async fn insert_log_lines(
    db_pool: &DatabaseConnection,
    logs: Vec<DatabaseFileLogLine>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ids: Vec<i64> = logs.iter().map(|item| item.id).collect();

    if ids.is_empty() {
        return Ok(());
    }

    let existing_ids: BTreeSet<i64> = log::Entity::find()
        .select_only()
        .column(log::Column::Id)
        .filter(log::Column::Id.is_in(ids))
        .into_tuple::<i64>()
        .all(db_pool)
        .await?
        .into_iter()
        .collect();

    for item in logs {
        if existing_ids.contains(&item.id) {
            continue;
        }

        let active_model = log::ActiveModel {
            id: Set(item.id),
            project: Set(item.project),
            machine_name: Set(item.machine_name),
            level: Set(item.level),
            category: Set(item.category),
            content: Set(item.content),
            create_time: Set(item.create_time.into()),
            delete_time: Set(None),
        };

        active_model.insert(db_pool).await?;
    }

    Ok(())
}

/// 恢复异常中断残留的数据库临时日志文件
async fn recover_tmp_files(
    base_path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !fs::try_exists(base_path).await? {
        return Ok(());
    }

    let mut entries = fs::read_dir(base_path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let path_text = path.to_string_lossy();

        if !path_text.ends_with(".log.tmp") || !path_text.contains("batch-") {
            continue;
        }

        let metadata = fs::metadata(&path).await?;
        let modified_time: DateTime<Utc> = metadata.modified()?.into();
        let age_seconds = (Utc::now() - modified_time).num_seconds();

        if age_seconds < TMP_RECOVER_AGE_SECONDS {
            continue;
        }

        let final_path = PathBuf::from(path_text.trim_end_matches(".tmp"));

        if fs::try_exists(&final_path).await? {
            let _ = fs::remove_file(&path).await;
        } else {
            let _ = fs::rename(&path, &final_path).await;
        }
    }

    Ok(())
}

/// 判断是否为数据库批量日志文件
fn is_database_batch_log_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    file_name.starts_with("batch-") && file_name.ends_with(".log")
}

/// 生成临时文件后缀
fn uuid_like_suffix() -> String {
    format!("{:x}", Utc::now().timestamp_nanos_opt().unwrap_or_default())
}
