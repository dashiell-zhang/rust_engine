use std::path::PathBuf;

use sea_orm::sqlx::postgres::PgConnectOptions;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpoolFileKind {
    Batch,
    Temporary,
    Rejected,
}

/// 文件消费、恢复和统计共用严格命名规则，不处理相似名称或其他组件文件。
pub(crate) fn file_kind(path: &std::path::Path) -> Option<SpoolFileKind> {
    let name = path.file_name()?.to_str()?;
    if batch_name(name) {
        Some(SpoolFileKind::Batch)
    } else if name.strip_suffix(".tmp").is_some_and(normal_batch_name) {
        Some(SpoolFileKind::Temporary)
    } else if name.strip_suffix(".rejected").is_some_and(batch_name) {
        Some(SpoolFileKind::Rejected)
    } else {
        None
    }
}

fn batch_name(name: &str) -> bool {
    normal_batch_name(name)
        || name
            .strip_prefix("batch-recovered-")
            .and_then(|name| name.strip_suffix(".log"))
            .is_some_and(guid)
}

fn guid(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn normal_batch_name(name: &str) -> bool {
    if name.len() != 60
        || !name.is_ascii()
        || !name.starts_with("batch-")
        || !name.ends_with(".log")
        || name.as_bytes()[23] != b'-'
    {
        return false;
    }
    let timestamp = &name[6..23];
    timestamp.bytes().all(|byte| byte.is_ascii_digit())
        && &timestamp[..4] != "0000"
        && chrono::NaiveDateTime::parse_from_str(timestamp, "%Y%m%d%H%M%S%3f").is_ok()
        && &timestamp[12..14] != "60"
        && guid(&name[24..56])
}

/// 同目录发布文件且不覆盖已有目标；中断留下的两份文件由稳定 ID 去重。
pub(crate) async fn publish_file(
    source: &std::path::Path,
    target: &std::path::Path,
) -> std::io::Result<()> {
    tokio::fs::hard_link(source, target).await?;
    tokio::fs::remove_file(source).await
}

/// 存储目标和项目共同确定积压目录；不把密码写入目录名或错误信息。
pub fn spool_directory(connection: &str, identity: &str, project: &str) -> Result<PathBuf, String> {
    if !connection.starts_with("postgres://") && !connection.starts_with("postgresql://") {
        return Err("日志数据库必须使用 PostgreSQL 连接 URL".to_string());
    }
    // 使用与数据库驱动一致的解析结果，包括 URL 查询参数覆盖与默认数据库名。
    // 密码、TLS 证书、池大小等不会改变日志存储归属，不参与目录标识。
    let options: PgConnectOptions = connection
        .parse()
        .map_err(|_| "日志数据库连接 URL 无效".to_string())?;
    let target = serde_json::to_vec(&(
        identity,
        project,
        options.get_host(),
        options.get_port(),
        options.get_socket(),
        options.get_database().unwrap_or(options.get_username()),
        options.get_username(),
        options.get_options(),
    ))
    .map_err(|_| "日志目标标识序列化失败".to_string())?;
    let key = Sha256::digest(target)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(logger::get_log_base_path().join("Database").join(key))
}
