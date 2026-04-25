use sea_orm::{DbBackend, Statement};

/// 强制主库注释前缀
pub const FORCE_PRIMARY_COMMENT: &str = "/*FORCE_PRIMARY*/ ";

/// PostgreSQL 返回 xmin 原始片段
pub const POSTGRES_RETURNING_XMIN_SQL: &str = "RETURNING xmin;";

/// PostgreSQL 返回 xmin 修补片段
pub const POSTGRES_RETURNING_PATCHED_SQL: &str = "RETURNING oid(txid_current()) as xmin;";

/// SQL 命令策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqlCommandPolicy {
    /// 是否强制主库执行
    pub force_primary: bool,

    /// 是否启用 PostgreSQL 语句修补
    pub patch_postgres: bool,
}

impl SqlCommandPolicy {
    /// 创建默认 PostgreSQL 命令策略
    pub const fn postgres() -> Self {
        Self {
            force_primary: false,
            patch_postgres: true,
        }
    }

    /// 创建强制主库的 PostgreSQL 命令策略
    pub const fn postgres_force_primary() -> Self {
        Self {
            force_primary: true,
            patch_postgres: true,
        }
    }
}

/// 判断 SQL 是否已显式标记主库执行
pub fn is_force_primary_sql(sql: &str) -> bool {
    sql.trim_start()
        .starts_with(FORCE_PRIMARY_COMMENT.trim_end())
}

/// 判断 SQL 是否需要 PostgreSQL 兼容性修补
pub fn needs_postgres_patch(sql: &str) -> bool {
    sql.contains(POSTGRES_RETURNING_XMIN_SQL)
}

/// 为 PostgreSQL SQL 应用兼容性修补
pub fn patch_postgres_sql(sql: impl AsRef<str>) -> String {
    let sql = sql.as_ref();

    if !needs_postgres_patch(sql) {
        return sql.to_string();
    }

    sql.replace(POSTGRES_RETURNING_XMIN_SQL, POSTGRES_RETURNING_PATCHED_SQL)
}

/// 为原始 SQL 增加强制主库前缀
fn mark_force_primary_sql(sql: impl AsRef<str>) -> String {
    let sql = sql.as_ref().trim_start();

    if is_force_primary_sql(sql) {
        return sql.to_string();
    }

    format!("{FORCE_PRIMARY_COMMENT}{sql}")
}

/// 按策略统一处理 PostgreSQL 原始 SQL
pub fn apply_postgres_sql_policy(sql: impl AsRef<str>, policy: SqlCommandPolicy) -> String {
    let mut sql = sql.as_ref().to_string();

    if policy.patch_postgres {
        sql = patch_postgres_sql(&sql);
    }

    if policy.force_primary {
        sql = mark_force_primary_sql(sql);
    }

    sql
}

/// 构建默认 PostgreSQL 策略的 Statement
pub fn build_postgres_statement(sql: impl Into<String>) -> Statement {
    Statement::from_string(
        DbBackend::Postgres,
        apply_postgres_sql_policy(sql.into(), SqlCommandPolicy::postgres()),
    )
}

/// 构建默认 PostgreSQL 策略的未预编译 SQL
pub fn build_postgres_unprepared_sql(sql: impl AsRef<str>) -> String {
    apply_postgres_sql_policy(sql, SqlCommandPolicy::postgres())
}
