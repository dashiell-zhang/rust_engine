//! 与数据库无关的迁移生成入口，写入可审查的固定操作、目标模型和登记文件。
use super::definition;
use crate::settings::DatabaseTarget;
use repository_core::migration::model::{ModelSnapshot, changes};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    format: u32,
    target: String,
    migration_id: String,
    model: ModelSnapshot,
}

fn target_name(target: DatabaseTarget) -> &'static str {
    match target {
        DatabaseTarget::Business => "business",
        DatabaseTarget::Log => "log",
        DatabaseTarget::EventBus => "event-bus",
    }
}

fn current_model(target: DatabaseTarget) -> Result<ModelSnapshot, String> {
    match target {
        DatabaseTarget::Business => crate::schema::model_snapshot(),
        DatabaseTarget::Log => logger_database::schema::model_snapshot(),
        DatabaseTarget::EventBus => event_bus_database::schema::model_snapshot(),
    }
}

fn baseline(target: DatabaseTarget) -> &'static str {
    match target {
        DatabaseTarget::Business => include_str!("generated/baseline.json"),
        DatabaseTarget::Log => logger_database::schema::MODEL_BASELINE,
        DatabaseTarget::EventBus => event_bus_database::schema::MODEL_BASELINE,
    }
}

fn output_directory(target: DatabaseTarget) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    match target {
        DatabaseTarget::Business => root.join("src/migrator/generated"),
        DatabaseTarget::Log => {
            root.join("../infrastructure/logger_database/src/migrations/generated")
        }
        DatabaseTarget::EventBus => {
            root.join("../infrastructure/event_bus_database/src/migrations/generated")
        }
    }
}

fn previous_snapshot(target: DatabaseTarget) -> Result<Snapshot, String> {
    let (_, migrations) = definition(target);
    let latest = migrations.last().ok_or("所选模型没有迁移登记")?;
    let snapshot: Snapshot =
        serde_json::from_str(latest.model_snapshot.unwrap_or_else(|| baseline(target)))
            .map_err(|e| format!("模型快照读取失败：{e}"))?;
    if snapshot.format != 1
        || snapshot.target != target_name(target)
        || snapshot.migration_id != latest.id()
    {
        return Err("模型快照与已编译迁移链不一致，请核对生成文件并重新编译".into());
    }
    Ok(snapshot)
}

/// 导出当前声明，便于审查；不会自动替换历史基线或连接数据库。
pub fn snapshot(target: DatabaseTarget) -> Result<String, String> {
    let (_, migrations) = definition(target);
    pretty(&Snapshot {
        format: 1,
        target: target_name(target).into(),
        migration_id: migrations
            .last()
            .map(|m| m.id())
            .unwrap_or_else(|| "0".into()),
        model: current_model(target)?,
    })
}

pub fn has_pending_changes(target: DatabaseTarget) -> Result<bool, String> {
    Ok(previous_snapshot(target)?.model != current_model(target)?)
}

/// 初始模型的完整离线 SQL 供设计时审查，不作为已有迁移链执行。
pub fn model_script(target: DatabaseTarget) -> Result<String, String> {
    let operations = pretty(&changes(
        &ModelSnapshot::default(),
        &current_model(target)?,
    )?)?;
    let statements = repository_core::migration::model::statements(&operations)?;
    Ok(format!(
        "BEGIN;\n\n{}\nCOMMIT;\n",
        statements
            .into_iter()
            .map(|s| format!("{};\n", s.trim_end_matches(';')))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

pub fn add(target: DatabaseTarget, name: &str, output: Option<&Path>) -> Result<PathBuf, String> {
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("迁移名称只能包含字母、数字和下划线".into());
    }
    let (_, migrations) = definition(target);
    if migrations.iter().any(|m| m.name.eq_ignore_ascii_case(name)) {
        return Err(format!("迁移名称已存在：{name}"));
    }
    let old = previous_snapshot(target)?;
    let new = current_model(target)?;
    let destructive = old.model.tables.iter().any(|(key, table)| {
        new.tables.get(key).is_none_or(|n| {
            table
                .columns
                .iter()
                .any(|c| !n.columns.iter().any(|nc| nc.name == c.name))
        })
    });
    let up = changes(&old.model, &new)?;
    let down = changes(&new, &old.model)?;
    let directory = output
        .map(Path::to_owned)
        .unwrap_or_else(|| output_directory(target));
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let lock_path = directory.join(".generation.lock");
    let _lock = GenerationLock::acquire(lock_path)?;
    let generated = migrations
        .iter()
        .filter(|m| m.model_snapshot.is_some())
        .map(|m| m.id())
        .collect::<Vec<_>>();
    let expected_index = index_source(&generated);
    let index_path = directory.join("index.rs");
    if index_path.exists() {
        let actual = fs::read_to_string(&index_path).map_err(|e| e.to_string())?;
        if compact(&actual) != compact(&expected_index) {
            return Err(
                "磁盘迁移登记与当前工具不一致；生成后请先重新编译，不得覆盖未编译的迁移".into(),
            );
        }
    } else if !generated.is_empty() {
        return Err("输出目录缺少已登记的生成迁移，请复制完整目录或使用默认位置".into());
    }
    let version = chrono::Utc::now()
        .format("%Y%m%d%H%M%S")
        .to_string()
        .parse::<i64>()
        .map_err(|e| e.to_string())?
        .max(migrations.last().map_or(0, |m| m.version + 1));
    let id = format!("{version}_{name}");
    let destination = directory.join(&id);
    // create_dir 拒绝覆盖同名产物；登记文件最后替换，失败不会登记半份迁移。
    fs::create_dir(&destination).map_err(|e| format!("创建迁移目录失败：{e}"))?;
    let result = (|| {
        fs::write(destination.join("up.json"), pretty(&up)?).map_err(|e| e.to_string())?;
        fs::write(destination.join("down.json"), pretty(&down)?).map_err(|e| e.to_string())?;
        fs::write(
            destination.join("model.json"),
            pretty(&Snapshot {
                format: 1,
                target: target_name(target).into(),
                migration_id: id.clone(),
                model: new,
            })?,
        )
        .map_err(|e| e.to_string())?;
        let source = format!(
            "repository_core::migration::Migration {{\n    version: {version},\n    name: {name:?},\n    build_statements: |_| repository_core::migration::model::statements(include_str!(\"up.json\")),\n    build_down_statements: |_| repository_core::migration::model::statements(include_str!(\"down.json\")),\n    model_snapshot: Some(include_str!(\"model.json\")),\n}}\n"
        );
        fs::write(destination.join("migration.rs"), source).map_err(|e| e.to_string())?;
        let mut ids = generated;
        ids.push(id.clone());
        let temporary = directory.join(format!(".{id}.index.tmp"));
        fs::write(&temporary, index_source(&ids)).map_err(|e| e.to_string())?;
        fs::rename(&temporary, &index_path).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        // 只清理本次 create_dir 成功的新目录；不会删除已有迁移。
        let _ = fs::remove_dir_all(&destination);
        let _ = fs::remove_file(directory.join(format!(".{id}.index.tmp")));
        return Err(error);
    }
    eprintln!(
        "已生成 {id}：Up {} 项，Down {} 项。审查后重新编译，再单独执行迁移。",
        up.len(),
        down.len()
    );
    if destructive {
        eprintln!("包含删除表或列的操作，可能丢失数据，请审查生成内容。");
    }
    Ok(destination)
}

fn index_source(ids: &[String]) -> String {
    if ids.is_empty() {
        return "&[]\n".into();
    }
    format!(
        "&[\n{}]\n",
        ids.iter()
            .map(|id| format!("    include!({:?}),\n", format!("{id}/migration.rs")))
            .collect::<String>()
    )
}
fn pretty(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map(|s| s + "\n")
        .map_err(|e| e.to_string())
}

fn compact(value: &str) -> String {
    value.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

struct GenerationLock {
    path: PathBuf,
    _file: fs::File,
}
impl GenerationLock {
    fn acquire(path: PathBuf) -> Result<Self, String> {
        let file = fs::OpenOptions::new().write(true).create_new(true).open(&path)
            .map_err(|e| format!("无法获取迁移生成锁 {}：{e}；若上次进程异常退出，请确认没有生成进程后清理该锁文件", path.display()))?;
        Ok(Self { path, _file: file })
    }
}
impl Drop for GenerationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
