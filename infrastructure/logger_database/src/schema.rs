use sea_orm::DbBackend;

use crate::log;

pub const MIGRATION_HISTORY_TABLE: &str = "__EFMigrationsHistory_Log";

/// 当前日志关系模型，分区周期保持为元数据，不包含当前月份的子表名。
pub fn model_snapshot() -> Result<repository_core::migration::model::ModelSnapshot, String> {
    use repository_core::migration::model::{ModelSnapshot, TableModel};
    use repository_core::partition::PartitionedModel;
    let mut table = TableModel::from_entity(log::Entity)?;
    table.add_audit_indexes()?;
    table.partition = Some(log::Model::partition_definition());
    let mut model = ModelSnapshot::default();
    model.add_table(table)?;
    Ok(model)
}

/// 工具目录比较只包含日志模型拥有的父表。
pub fn json_index_tables() -> Vec<repository_core::json_index::JsonIndexTable> {
    use repository_core::partition::PartitionedModel;
    let mut table = repository_core::json_index::JsonIndexTable::from_entity(log::Entity);
    table.partition_key = Some(log::Model::partition_definition().key_column);
    vec![table]
}

/// 日志模型独立维护 schema，不创建业务表或任务表。
pub fn initial_schema_statements(backend: DbBackend) -> Result<Vec<String>, String> {
    if backend != DbBackend::Postgres {
        return Err("日志分区模型只支持 PostgreSQL".into());
    }
    // 冻结迁移的父表和分区声明；初始子分区仍按 SQL 生成时刻计算，
    // 与源 NpgsqlPartitionMigrationsSqlGenerator 一致，不读取当前实体声明。
    let (snapshot, definition): (
        &[&str],
        repository_core::partition::PartitionTableDefinition,
    ) = include!("migrations/202611070001.rs");
    let mut statements = snapshot
        .iter()
        .map(|sql| (*sql).to_owned())
        .collect::<Vec<_>>();
    let range = definition.initial_range(chrono::Utc::now())?;
    statements.insert(1, definition.create_child_statement(&range));
    Ok(statements)
}

/// 固定初始迁移的撤销定义，不读取当前实体。
pub fn initial_schema_down_statements(_: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("migrations/202611070001.down.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

/// 所属组件统一持有生成的迁移，工具与宿主不能另建迁移链。
pub fn generated_migrations() -> &'static [repository_core::migration::Migration] {
    include!("migrations/generated/index.rs")
}

pub const MODEL_BASELINE: &str = include_str!("migrations/generated/baseline.json");
