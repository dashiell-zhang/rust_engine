use crate::schema;
use repository::sql_command::{build_postgres_statement, build_postgres_unprepared_sql};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, FromQueryResult, Statement};

/// 单个数据库迁移定义
struct Migration {
    /// 迁移版本号
    version: i64,

    /// 迁移名称
    name: &'static str,

    /// 迁移语句构建器
    build_statements: fn(DbBackend) -> Vec<String>,
}

/// 返回全部迁移定义
fn all_migrations() -> &'static [Migration] {
    &[
        Migration {
            version: 202604050001,
            name: "initial_schema_code_first",
            build_statements: schema::initial_schema_statements,
        },
        Migration {
            version: 202604050002,
            name: "align_legacy_schema_to_code_first",
            build_statements: schema::legacy_alignment_statements,
        },
    ]
}

/// 迁移历史表名
const MIGRATION_HISTORY_TABLE: &str = "__MigrationHistory";

/// 执行数据库迁移
pub async fn migrate(db: &DatabaseConnection) -> Result<(), String> {
    ensure_migration_history_table(db).await?;

    let applied_versions = load_applied_versions(db).await?;
    let backend = db.get_database_backend();

    for migration in all_migrations() {
        if applied_versions.contains(&migration.version) {
            continue;
        }

        println!("正在执行迁移 {} {}", migration.version, migration.name);

        for statement in (migration.build_statements)(backend) {
            if statement.trim().is_empty() {
                continue;
            }

            let statement = build_postgres_unprepared_sql(statement);

            db.execute_unprepared(&statement).await.map_err(|error| {
                format!(
                    "执行数据库迁移 {} 失败: {error}\nSQL: {statement}",
                    migration.version
                )
            })?;
        }

        let insert_statement = Statement::from_sql_and_values(
            DbBackend::Postgres,
            format!(
                "INSERT INTO \"{MIGRATION_HISTORY_TABLE}\" (\"Version\", \"Name\") VALUES ($1, $2)"
            ),
            [migration.version.into(), migration.name.into()],
        );

        db.execute(insert_statement).await.map_err(|error| {
            format!(
                "写入迁移历史 {} {} 失败: {error}",
                migration.version, migration.name
            )
        })?;
    }

    Ok(())
}

/// 确保迁移历史表存在
async fn ensure_migration_history_table(db: &DatabaseConnection) -> Result<(), String> {
    let statement = build_postgres_unprepared_sql(format!(
        r#"
CREATE TABLE IF NOT EXISTS "{MIGRATION_HISTORY_TABLE}" (
    "Version" BIGINT PRIMARY KEY,
    "Name" TEXT NOT NULL,
    "CreateTime" TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
"#
    ));

    db.execute_unprepared(&statement)
        .await
        .map_err(|error| format!("创建迁移历史表失败: {error}"))?;

    Ok(())
}

/// 加载已执行的迁移版本
async fn load_applied_versions(db: &DatabaseConnection) -> Result<Vec<i64>, String> {
    #[derive(sea_orm::FromQueryResult)]
    /// 已应用迁移记录
    struct AppliedMigration {
        /// 已执行版本号
        #[sea_orm(column_name = "Version")]
        version: i64,
    }

    let sql = format!(r#"SELECT "Version" FROM "{MIGRATION_HISTORY_TABLE}" ORDER BY "Version""#);

    AppliedMigration::find_by_statement(build_postgres_statement(sql))
        .all(db)
        .await
        .map(|items| items.into_iter().map(|item| item.version).collect())
        .map_err(|error| format!("读取迁移历史失败: {error}"))
}
