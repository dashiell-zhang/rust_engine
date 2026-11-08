use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, FromQueryResult, Statement, TransactionTrait,
};

pub mod model;
mod script;
pub use script::{ScriptOptions, create_script};

const PRODUCT_VERSION: &str = concat!("rust_engine/", env!("CARGO_PKG_VERSION"));

/// 单个写模型的迁移定义；组件拥有自己的序列和历史表。
#[derive(Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub build_statements: fn(DbBackend) -> Result<Vec<String>, String>,
    pub build_down_statements: fn(DbBackend) -> Result<Vec<String>, String>,
    /// 生成该迁移时保存的模型；旧迁移使用所属模型的已核验基线。
    pub model_snapshot: Option<&'static str>,
}

impl Migration {
    /// 完整迁移标识用于历史记录及脚本区间选择。
    pub fn id(&self) -> String {
        format!("{}_{}", self.version, self.name)
    }
}

fn validate_history_table(history_table: &str) -> Result<(), String> {
    if history_table.is_empty()
        || !history_table
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("迁移历史表标识无效".to_string());
    }
    Ok(())
}

fn create_history_sql(history_table: &str) -> String {
    format!(
        r#"CREATE TABLE IF NOT EXISTS "public"."{history_table}" (
        "MigrationId" VARCHAR(150) PRIMARY KEY,
        "ProductVersion" VARCHAR(32) NOT NULL
    )"#
    )
}

/// 在事务与数据库级锁内应用所选模型的迁移，不启动任何宿主后台服务。
pub async fn migrate(
    db: &DatabaseConnection,
    history_table: &str,
    migrations: &[Migration],
    to: Option<&str>,
) -> Result<(), String> {
    validate_history_table(history_table)?;
    let target = resolve_target(migrations, to)?;
    if db.get_database_backend() != DbBackend::Postgres {
        return Err("当前迁移仅支持 PostgreSQL".to_string());
    }
    let transaction = db.begin().await.map_err(|error| error.to_string())?;
    transaction
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT pg_advisory_xact_lock(hashtext(current_database()), hashtext($1))",
            [history_table.into()],
        ))
        .await
        .map_err(|error| format!("获取迁移锁失败: {error}"))?;
    transaction
        .execute_unprepared(&create_history_sql(history_table))
        .await
        .map_err(|error| format!("创建迁移历史表失败: {error}"))?;

    #[derive(FromQueryResult)]
    struct AppliedMigration {
        migration_id: String,
    }
    let applied = AppliedMigration::find_by_statement(Statement::from_string(
        DbBackend::Postgres,
        format!(r#"SELECT "MigrationId" AS migration_id FROM "public"."{history_table}""#),
    ))
    .all(&transaction)
    .await
    .map_err(|error| error.to_string())?;
    // 先逆序撤销目标之后已应用的步骤，再补齐目标之前尚未应用的步骤。
    // 未登记在本迁移链中的历史记录不由本模型操作。
    let steps = migrations[target..]
        .iter()
        .rev()
        .map(|migration| (migration, true))
        .chain(
            migrations[..target]
                .iter()
                .map(|migration| (migration, false)),
        );
    for (migration, down) in steps {
        let migration_id = migration.id();
        let exists = applied.iter().any(|item| item.migration_id == migration_id);
        if exists != down {
            continue;
        }
        let action = if down { "回退" } else { "执行" };
        println!("正在{action}迁移 {} {}", migration.version, migration.name);
        let build = if down {
            migration.build_down_statements
        } else {
            migration.build_statements
        };
        for sql in build(DbBackend::Postgres)? {
            if sql.trim().is_empty() {
                continue;
            }
            transaction
                .execute_unprepared(&sql)
                .await
                .map_err(|error| {
                    format!(
                        "{action}迁移 {} {} 失败: {error}",
                        migration.version, migration.name
                    )
                })?;
        }
        let history = if down {
            Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(r#"DELETE FROM "public"."{history_table}" WHERE "MigrationId" = $1"#),
                [migration_id.into()],
            )
        } else {
            Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    r#"INSERT INTO "public"."{history_table}" ("MigrationId", "ProductVersion") VALUES ($1, $2)"#
                ),
                [migration_id.into(), PRODUCT_VERSION.into()],
            )
        };
        transaction
            .execute_raw(history)
            .await
            .map_err(|error| format!("写入迁移历史失败: {error}"))?;
    }
    transaction
        .commit()
        .await
        .map_err(|error| format!("提交数据库迁移失败: {error}"))
}

/// 解析所选模型的目标；0 表示未应用迁移，省略时选择最新版本。
/// 工具可在连接数据库前调用，执行与离线导出使用相同的区间规则。
pub fn resolve_target(migrations: &[Migration], value: Option<&str>) -> Result<usize, String> {
    let Some(value) = value else {
        return Ok(migrations.len());
    };
    if value == "0" {
        return Ok(0);
    }
    let matches = migrations
        .iter()
        .enumerate()
        .filter(|(_, migration)| {
            migration.id().eq_ignore_ascii_case(value)
                || migration.name.eq_ignore_ascii_case(value)
                || migration.version.to_string() == value
        })
        .map(|(index, _)| index + 1)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => Err(format!("所选模型没有迁移：{value}")),
        _ => Err(format!("迁移名称不唯一，请使用完整标识：{value}")),
    }
}
