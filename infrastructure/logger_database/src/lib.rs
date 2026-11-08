use repository_core::versioned::insert_many_versioned;
pub mod diagnostics;
mod file_format;
pub mod query;
pub mod schema;
pub mod spool;
mod writer;
use file_format::DatabaseFileLogLine;

use std::{
    collections::BTreeSet,
    fs as std_fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use common::identifier_generator::IdentifierGenerator;
use logger::{LoggerRuntime, LoggerSettings, init_logger_subscriber_with_sink};

use repository_core::partition::{PartitionMaintenance, PartitionedModel};
/// 数据库日志实体，由日志组件独立维护。
pub mod log;
use diagnostics::Operation;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, ExprTrait, QueryFilter,
    QuerySelect, TransactionTrait, sea_query::Expr,
};
use spool::SpoolFileKind;
use tokio::{
    fs::{self, File},
    io::{AsyncBufReadExt, BufReader},
    time::{self, Duration},
};

const LOG_FILE_SCAN_LIMIT: usize = 50;
const SAVE_INTERVAL_SECONDS: u64 = 5;
const TMP_RECOVER_AGE_SECONDS: i64 = 300;

/// 初始化数据库日志组件
///
/// 设置全局 subscriber，启动写盘、入库与清理任务。
/// 正常退出应调用运行时的 shutdown 排空日志；Drop 取消后台操作。
pub async fn init_database_logger(
    settings: &LoggerSettings,
    db_pool: Arc<DatabaseConnection>,
    identifier_generator: IdentifierGenerator,
    connection: &str,
    maintenance_lock: Arc<dyn distributed_lock::DistributedLock>,
) -> Result<LoggerRuntime, String> {
    settings.validate_log_filter(logger::LoggerProvider::Database)?;
    if settings.project.trim().is_empty() || settings.database_identity.trim().is_empty() {
        return Err("日志项目和存储标识不可以为空".to_string());
    }
    if settings.save_days != -1 && settings.save_days <= 0 {
        return Err("日志保留天数必须为正数或 -1".to_string());
    }
    settings.database_buffer.validate()?;
    let maintenance = PartitionMaintenance::new(
        Arc::clone(&db_pool),
        maintenance_lock,
        settings.database_identity.clone(),
        vec![log::Model::partition_definition()],
    )?
    .start()
    .await;
    let base_path =
        spool::spool_directory(connection, &settings.database_identity, &settings.project)?;
    std_fs::create_dir_all(&base_path)
        .map_err(|error| format!("创建数据库日志目录失败: {error}"))?;
    let diagnostics = diagnostics::Diagnostics::new();
    let writer = writer::DatabaseLogWriter::start(
        settings.database_buffer.clone(),
        identifier_generator,
        base_path.clone(),
        diagnostics.clone(),
    );
    let filter_handle = init_logger_subscriber_with_sink(
        settings,
        writer.clone(),
        logger::LoggerProvider::Database,
    )?;
    diagnostics.publish();
    let save_handle = tokio::spawn(run_log_save_task(
        Arc::clone(&db_pool),
        base_path,
        diagnostics.clone(),
    ));
    let clear_handle = tokio::spawn(run_database_log_clear_task(
        Arc::clone(&db_pool),
        settings.save_days,
        settings.project.clone(),
        diagnostics,
    ));
    LoggerRuntime::new_async(writer, Some(save_handle), Some(clear_handle))
        .with_filter_handle(filter_handle)
        .with_background_guard(maintenance)
        .with_filter_reload(settings)
}

/// 数据库日志入库任务
async fn run_log_save_task(
    db_pool: Arc<DatabaseConnection>,
    base_path: PathBuf,
    diagnostics: Arc<diagnostics::Diagnostics>,
) {
    let mut next_maintenance = time::Instant::now();
    loop {
        if time::Instant::now() >= next_maintenance {
            if let Err(error) = recover_tmp_files(&base_path, &diagnostics).await {
                diagnostics.failure(Operation::Recovery, error);
            }
            if let Err(error) = refresh_spool(&base_path, &diagnostics).await {
                diagnostics.failure(Operation::Database, error);
            }
            next_maintenance = time::Instant::now() + Duration::from_secs(30);
        }
        match save_logs_to_database(&db_pool, &base_path, &diagnostics).await {
            Ok(processed) if processed > 0 => continue,
            Ok(_) => {}
            Err(error) => diagnostics.failure(Operation::Database, error),
        }
        time::sleep(Duration::from_secs(SAVE_INTERVAL_SECONDS)).await;
    }
}

/// 数据库日志清理任务
async fn run_database_log_clear_task(
    db_pool: Arc<DatabaseConnection>,
    save_days: i64,
    project: String,
    diagnostics: Arc<diagnostics::Diagnostics>,
) {
    if save_days == -1 {
        return;
    }

    // 源 Npgsql 将 DateTimeOffset.MinValue 编码为 -infinity，不能用有限的公元 1 年替代。
    const MINIMUM: DateTime<Utc> =
        DateTime::from_timestamp(-62_135_596_800, 0).expect("0001-01-01 UTC");
    loop {
        let delete_time = chrono::Duration::try_days(save_days)
            .and_then(|duration| Utc::now().checked_sub_signed(duration))
            .filter(|cutoff| *cutoff > MINIMUM)
            .map_or_else(|| Expr::cust("'-infinity'::timestamptz"), Expr::value);

        if let Err(error) = log::Entity::delete_many()
            .filter(log::Column::Project.eq(&project))
            .filter(log::Column::DeleteTime.is_null())
            .filter(Expr::col(log::Column::CreateTime).lte(delete_time))
            .exec(&*db_pool)
            .await
        {
            diagnostics.failure(Operation::Cleanup, error);
        }

        time::sleep(Duration::from_secs(60 * 60 * 24)).await;
    }
}

/// 扫描数据库日志文件并写入数据库
async fn save_logs_to_database(
    db_pool: &DatabaseConnection,
    base_path: &Path,
    diagnostics: &diagnostics::Diagnostics,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let mut entries = fs::read_dir(base_path).await?;
    let mut processed = 0;
    while let Some(entry) = entries.next_entry().await? {
        let log_path = entry.path();
        if spool::file_kind(&log_path) != Some(SpoolFileKind::Batch)
            || !entry.file_type().await?.is_file()
        {
            continue;
        }
        let (logs, corrupted) = read_database_log_lines(&log_path).await?;
        if !logs.is_empty() {
            insert_log_lines(db_pool, logs).await?;
        }
        if corrupted {
            spool::publish_file(&log_path, &log_path.with_extension("log.rejected")).await?;
            diagnostics.failure(Operation::Quarantine, "日志文件包含损坏行");
        } else {
            fs::remove_file(&log_path).await?;
        }
        processed += 1;
        if processed >= LOG_FILE_SCAN_LIMIT {
            break;
        }
    }
    Ok(processed)
}

/// 读取单个数据库日志文件
async fn read_database_log_lines(
    path: &Path,
) -> Result<(Vec<DatabaseFileLogLine>, bool), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path).await?;
    let mut reader = BufReader::with_capacity(65536, file);
    let mut line = Vec::with_capacity(4096);
    let mut logs = Vec::new();
    let mut corrupted = false;
    let mut first_line = true;
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            if !line.is_empty() {
                parse_log_line(&line, first_line, &mut logs, &mut corrupted);
            }
            break;
        }
        // CR、LF 均为分隔符；CRLF 多出的一条空行被忽略，也支持跨缓冲边界。
        let end = buffer.iter().position(|byte| matches!(byte, b'\r' | b'\n'));
        let count = end.unwrap_or(buffer.len());
        line.extend_from_slice(&buffer[..count]);
        let consumed = count + usize::from(end.is_some());
        reader.consume(consumed);
        if end.is_some() {
            parse_log_line(&line, first_line, &mut logs, &mut corrupted);
            first_line = false;
            if line.capacity() > 65536 {
                line = Vec::with_capacity(4096);
            } else {
                line.clear();
            }
        }
    }
    Ok((logs, corrupted))
}

fn parse_log_line(
    bytes: &[u8],
    first: bool,
    logs: &mut Vec<DatabaseFileLogLine>,
    corrupted: &mut bool,
) {
    let bytes = if first {
        bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)
    } else {
        bytes
    };
    let text = String::from_utf8_lossy(bytes);
    if text.trim().is_empty() {
        return;
    }
    match serde_json::from_str::<DatabaseFileLogLine>(&text) {
        Ok(log) if log.id > 0 => logs.push(log),
        _ => *corrupted = true,
    }
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

    let transaction = db_pool.begin().await?;
    // 恢复日志的幂等去重必须包含已软删除记录，避免重放同一主键。
    let mut existing_ids: BTreeSet<i64> = log::Entity::find_with_deleted()
        .select_only()
        .column(log::Column::Id)
        .filter(log::Column::Id.is_in(ids))
        .into_tuple::<i64>()
        .all(&transaction)
        .await?
        .into_iter()
        .collect();

    let mut pending = Vec::new();
    for item in logs {
        if !existing_ids.insert(item.id) {
            continue;
        }

        let active_model = log::ActiveModel {
            row_version: Default::default(),
            id: Set(item.id),
            project: Set(item.project),
            machine_name: Set(item.machine_name),
            level: Set(item.level),
            category: Set(item.category),
            content: Set(item.content),
            create_time: Set(item.create_time.into()),
            delete_time: Set(item.delete_time.map(Into::into)),
        };

        pending.push(active_model);
    }
    insert_many_versioned(pending, &transaction).await?;
    transaction.commit().await?;
    Ok(())
}

/// 恢复超过五分钟且没有写入者持有的临时文件，不覆盖同名批次。
async fn recover_tmp_files(
    base_path: &Path,
    diagnostics: &diagnostics::Diagnostics,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut entries = fs::read_dir(base_path).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if spool::file_kind(&path) != Some(SpoolFileKind::Temporary)
            || !entry.file_type().await?.is_file()
        {
            continue;
        }
        if let Err(error) = recover_tmp_file(path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            diagnostics.failure(Operation::Recovery, error);
        }
    }
    Ok(())
}

async fn recover_tmp_file(path: PathBuf) -> std::io::Result<()> {
    let lock_path = path.clone();
    let locked = tokio::task::spawn_blocking(move || -> std::io::Result<Option<std::fs::File>> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(lock_path)?;
        if file.metadata()?.modified()?.elapsed().unwrap_or_default()
            < Duration::from_secs(TMP_RECOVER_AGE_SECONDS as u64)
        {
            return Ok(None);
        }
        file.try_lock().map_err(std::io::Error::other)?;
        Ok(Some(file))
    })
    .await??;
    let Some(_lock) = locked else {
        return Ok(());
    };
    let final_path = path.with_extension("");
    match spool::publish_file(&path, &final_path).await {
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let recovered = path.with_file_name(format!(
                "batch-recovered-{}.log",
                uuid::Uuid::new_v4().simple()
            ));
            spool::publish_file(&path, &recovered).await
        }
        result => result,
    }
}

async fn refresh_spool(
    base_path: &Path,
    diagnostics: &diagnostics::Diagnostics,
) -> std::io::Result<()> {
    use std::sync::atomic::Ordering;
    let mut entries = fs::read_dir(base_path).await?;
    let (mut files, mut bytes, mut rejected) = (0u64, 0u64, 0u64);
    while let Some(entry) = entries.next_entry().await? {
        let Some(kind) = spool::file_kind(&entry.path()) else {
            continue;
        };
        if !entry.file_type().await?.is_file() {
            continue;
        }
        if kind == SpoolFileKind::Rejected {
            rejected += 1;
            continue;
        }
        match entry.metadata().await {
            Ok(metadata) => {
                files += 1;
                bytes = bytes.saturating_add(metadata.len());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    diagnostics.spool_files.store(files, Ordering::Relaxed);
    diagnostics.spool_bytes.store(bytes, Ordering::Relaxed);
    diagnostics
        .rejected_files
        .store(rejected, Ordering::Relaxed);
    Ok(())
}
