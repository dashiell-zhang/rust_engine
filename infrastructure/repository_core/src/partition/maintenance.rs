use super::{ExistingPartition, PartitionTableDefinition, partition_name};
use chrono::Utc;
use distributed_lock::DistributedLock;
use regex::Regex;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, FromQueryResult, Statement, TransactionTrait,
};
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

const LEASE: Duration = Duration::from_secs(600);
const RENEW_INTERVAL: Duration = Duration::from_secs(180);
static BOUND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r"(?i)^FOR\s+VALUES\s+FROM\s*\(\s*'?(-?\d+)'?(?:::[^)]*)?\s*\)\s+TO\s*\(\s*'?(-?\d+)'?(?:::[^)]*)?\s*\)$"
).expect("分区边界表达式")
});

pub struct PartitionMaintenance {
    db: Arc<DatabaseConnection>,
    lock: Arc<dyn DistributedLock>,
    database_identity: String,
    definitions: Vec<PartitionTableDefinition>,
}

/// 宿主持有守卫，释放时取消维护；事务回滚后归还连接。
pub struct PartitionMaintenanceRuntime(tokio::task::JoinHandle<()>);

impl Drop for PartitionMaintenanceRuntime {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl PartitionMaintenance {
    pub fn new(
        db: Arc<DatabaseConnection>,
        lock: Arc<dyn DistributedLock>,
        database_identity: String,
        definitions: Vec<PartitionTableDefinition>,
    ) -> Result<Self, String> {
        if database_identity.trim().is_empty() {
            return Err("分区维护数据库标识不能为空".into());
        }
        if db.get_database_backend() != DbBackend::Postgres {
            return Err("分区维护仅支持 PostgreSQL".into());
        }
        let mut seen = std::collections::HashSet::new();
        for definition in &definitions {
            definition.validate()?;
            if !seen.insert((definition.schema.clone(), definition.table.clone())) {
                return Err(format!("重复分区声明: {}", definition.qualified_table()));
            }
        }
        Ok(Self {
            db,
            lock,
            database_identity,
            definitions,
        })
    }

    /// 首轮检查完成后返回，错误记录后继续启动；后续每十分钟维护一次。
    pub async fn start(self) -> PartitionMaintenanceRuntime {
        self.ensure_and_report().await;
        PartitionMaintenanceRuntime(tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(600)).await;
                self.ensure_and_report().await;
            }
        }))
    }

    async fn ensure_and_report(&self) {
        if let Err(error) = self.ensure().await {
            // 使用标准错误输出，避免日志数据库故障导致递归写入自身。
            eprintln!("PostgreSQL 分区维护失败: {error}");
        }
    }

    pub async fn ensure(&self) -> Result<(), String> {
        for definition in &self.definitions {
            let parent = load_parent(self.db.as_ref(), definition).await?;
            let mut resolved = definition.clone();
            resolved.schema = Some(parent.schema_name.clone());
            let key = format!(
                "PostgreSql.PartitionMaintenance:{}:{}:{}:{}",
                self.database_identity, parent.database_name, parent.schema_name, definition.table
            );
            let Some(guard) = self
                .lock
                .try_lock(&key, LEASE, 1)
                .await
                .map_err(|error| error.to_string())?
            else {
                tracing::info!(table = %resolved.qualified_table(), "另一个实例正在维护分区，本轮跳过");
                continue;
            };
            let renew = async {
                loop {
                    tokio::time::sleep(RENEW_INTERVAL).await;
                    match tokio::time::timeout(RENEW_INTERVAL, self.lock.renew(&guard, LEASE)).await
                    {
                        Ok(Ok(true)) => {}
                        _ => return Err::<(), String>("分区维护锁续期失败，取消本轮事务".into()),
                    }
                }
            };
            let operation = self.ensure_table(&resolved);
            tokio::select! {
                biased;
                result = renew => result?,
                result = operation => result?,
            }
        }
        Ok(())
    }

    async fn ensure_table(&self, definition: &PartitionTableDefinition) -> Result<(), String> {
        // 事务取消或失锁时回滚，DDL 不会以半个维护批次提交。
        let transaction = self.db.begin().await.map_err(|error| error.to_string())?;
        transaction
            .execute_unprepared("SET LOCAL statement_timeout = '120s'")
            .await
            .map_err(|e| e.to_string())?;
        transaction
            .execute_unprepared("SET LOCAL lock_timeout = '60s'")
            .await
            .map_err(|e| e.to_string())?;
        load_parent(&transaction, definition).await?;
        let existing = load_children(&transaction, definition).await?;
        let required = definition.required_ranges(&existing, Utc::now())?;
        // 先验证名称与范围，再执行任何创建。
        for range in &required {
            let name = partition_name(&definition.table, range.start_time);
            if existing.iter().any(|p| {
                p.name == name || (p.start_id == range.start_id && p.end_id == range.end_id)
            }) {
                return Err(format!("分区 {name} 已有名称或范围与计划冲突"));
            }
        }
        for range in &required {
            // 不使用 IF NOT EXISTS，避免把同名的无关表误认为目标子分区。
            if let Err(error) = transaction
                .execute_unprepared(&definition.create_child_statement(range))
                .await
            {
                transaction
                    .rollback()
                    .await
                    .map_err(|rollback| format!("创建分区失败: {error}；回滚失败: {rollback}"))?;
                // 外部维护者可能恰好完成相同计划。只接受相同父表、schema、名称和边界。
                let refreshed = load_children(self.db.as_ref(), definition).await?;
                super::validate_ranges(&refreshed)?;
                if required.iter().all(|expected| {
                    refreshed.iter().any(|actual| {
                        actual.name == partition_name(&definition.table, expected.start_time)
                            && Some(actual.schema.as_str()) == definition.schema.as_deref()
                            && actual.start_id == expected.start_id
                            && actual.end_id == expected.end_id
                    })
                }) {
                    return Ok(());
                }
                return Err(format!(
                    "创建 {} 子分区失败: {error}",
                    definition.qualified_table()
                ));
            }
        }
        transaction
            .commit()
            .await
            .map_err(|error| format!("提交分区维护失败: {error}"))?;
        for range in required {
            tracing::info!(table = %definition.qualified_table(), start_id = range.start_id,
                end_id = range.end_id, "已创建 PostgreSQL 子分区");
        }
        Ok(())
    }
}

#[derive(FromQueryResult)]
struct Parent {
    schema_name: String,
    database_name: String,
    strategy: String,
    key_count: i16,
    direct_column: bool,
    key_name: Option<String>,
    bigint_key: Option<bool>,
}

async fn load_parent(
    db: &impl ConnectionTrait,
    definition: &PartitionTableDefinition,
) -> Result<Parent, String> {
    let parent = Parent::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT n.nspname::text AS schema_name, current_database()::text AS database_name,
                p.partstrat::text AS strategy, p.partnatts AS key_count,
                p.partexprs IS NULL AS direct_column, a.attname::text AS key_name,
                a.atttypid = 'pg_catalog.int8'::regtype AS bigint_key
         FROM pg_catalog.pg_partitioned_table p
         JOIN pg_catalog.pg_class c ON c.oid = p.partrelid
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
         LEFT JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid AND a.attnum = p.partattrs[0]
         WHERE c.oid = pg_catalog.to_regclass($1)",
        [definition.qualified_table().into()],
    ))
    .one(db)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| {
        format!(
            "{} 不存在或不是分区父表；不会自动转换普通表",
            definition.qualified_table()
        )
    })?;
    if parent.strategy != "r"
        || parent.key_count != 1
        || !parent.direct_column
        || parent.key_name.as_deref() != Some(&definition.key_column)
        || parent.bigint_key != Some(true)
    {
        return Err(format!(
            "{} 必须使用声明的单列 bigint RANGE 分区键",
            definition.qualified_table()
        ));
    }
    Ok(parent)
}

async fn load_children(
    db: &impl ConnectionTrait,
    definition: &PartitionTableDefinition,
) -> Result<Vec<ExistingPartition>, String> {
    #[derive(FromQueryResult)]
    struct Child {
        schema_name: String,
        table_name: String,
        kind: String,
        bound: String,
    }
    let rows = Child::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT n.nspname::text AS schema_name, c.relname::text AS table_name,
                c.relkind::text AS kind, pg_catalog.pg_get_expr(c.relpartbound, c.oid) AS bound
         FROM pg_catalog.pg_inherits i
         JOIN pg_catalog.pg_class c ON c.oid = i.inhrelid
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
         WHERE i.inhparent = pg_catalog.to_regclass($1)",
        [definition.qualified_table().into()],
    ))
    .all(db)
    .await
    .map_err(|error| error.to_string())?;
    rows.into_iter()
        .map(|row| {
            if row.kind != "r" {
                return Err(format!(
                    "{} 不是普通叶子分区，不支持多级或外部分区",
                    row.table_name
                ));
            }
            let captures = BOUND.captures(&row.bound).ok_or_else(|| {
                format!(
                    "{} 边界不是有限 bigint RANGE: {}",
                    row.table_name, row.bound
                )
            })?;
            Ok(ExistingPartition {
                schema: row.schema_name,
                name: row.table_name,
                start_id: captures[1].parse::<i64>().map_err(|e| e.to_string())?,
                end_id: captures[2].parse::<i64>().map_err(|e| e.to_string())?,
            })
        })
        .collect()
}
