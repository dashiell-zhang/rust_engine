use sea_orm::DbBackend;

use super::{
    Migration, PRODUCT_VERSION, create_history_sql, resolve_target, validate_history_table,
};

/// 起点表示已应用的迁移，终点表示要保留到的迁移。
/// 升级应用 (from, to]；回退逆序撤销 (to, from]。
#[derive(Default)]
pub struct ScriptOptions<'a> {
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
    pub idempotent: bool,
}

/// 离线生成所选模型的 SQL；只构建字符串，不创建连接或执行语句。
pub fn create_script(
    history_table: &str,
    migrations: &[Migration],
    options: ScriptOptions<'_>,
) -> Result<String, String> {
    validate_history_table(history_table)?;
    let from = resolve_target(migrations, Some(options.from.unwrap_or("0")))?;
    let to = resolve_target(migrations, options.to)?;
    if from == to {
        return Ok(String::new());
    }

    let mut script = String::from("BEGIN;\n\n");
    append_statement(
        &mut script,
        &format!(
            "SELECT pg_advisory_xact_lock(hashtext(current_database()), hashtext({}))",
            literal(history_table)
        ),
    );
    append_statement(&mut script, &create_history_sql(history_table));
    let down = from > to;
    let mut steps = migrations[from.min(to)..from.max(to)]
        .iter()
        .collect::<Vec<_>>();
    if down {
        steps.reverse();
    }
    for migration in steps {
        let migration_id = literal(&migration.id());
        let history = if down {
            format!(
                r#"DELETE FROM "public"."{history_table}" WHERE "MigrationId" = {migration_id}"#
            )
        } else {
            format!(
                r#"INSERT INTO "public"."{history_table}" ("MigrationId", "ProductVersion") VALUES ({migration_id}, {})"#,
                literal(PRODUCT_VERSION)
            )
        };
        let build = if down {
            migration.build_down_statements
        } else {
            migration.build_statements
        };
        let statements = build(DbBackend::Postgres)?;
        if options.idempotent {
            let condition = if down { "EXISTS" } else { "NOT EXISTS" };
            let mut body = format!(
                "BEGIN\n    IF {condition} (SELECT 1 FROM \"public\".\"{history_table}\" WHERE \"MigrationId\" = {migration_id}) THEN\n"
            );
            for sql in statements.iter().filter(|sql| !sql.trim().is_empty()) {
                // 动态执行使已应用迁移中的旧表名、旧列名不在跳过前被解析。
                body.push_str(&format!("        EXECUTE {};\n", literal(sql)));
            }
            body.push_str(&format!("        {history};\n    END IF;\nEND;"));
            let mut delimiter = String::from("$rust_engine_migration$");
            while body.contains(&delimiter) {
                delimiter.insert(delimiter.len() - 1, '_');
            }
            script.push_str(&format!("DO {delimiter}\n{body}\n{delimiter};\n\n"));
        } else {
            for sql in &statements {
                append_statement(&mut script, sql);
            }
            append_statement(&mut script, &history);
        }
    }
    script.push_str("COMMIT;\n");
    Ok(script)
}

fn append_statement(output: &mut String, sql: &str) {
    let sql = sql.trim();
    if sql.is_empty() {
        return;
    }
    output.push_str(sql);
    if !sql.ends_with(';') {
        output.push_str("\n;");
    }
    output.push_str("\n\n");
}

/// PostgreSQL E 字符串显式转义反斜线和单引号，不依赖会话字符串配置。
fn literal(value: &str) -> String {
    format!("E'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}
