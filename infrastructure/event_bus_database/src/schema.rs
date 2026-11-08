use sea_orm::DbBackend;
pub const MIGRATION_HISTORY_TABLE: &str = "__EFMigrationsHistory_EventBus";

/// 当前事件存储模型；该组件使用 SQL 存取，结构声明由组件自身持有。
pub fn model_snapshot() -> Result<repository_core::migration::model::ModelSnapshot, String> {
    use repository_core::migration::model::{ModelSnapshot, TableModel};
    use sea_orm::sea_query::{Alias, ColumnDef, Table};
    let mut model = ModelSnapshot::default();
    for name in ["EventBusStream", "EventBusMessage"] {
        let mut table = Table::create();
        table
            .table(Alias::new(name))
            .comment(if name == "EventBusMessage" {
                "保存可由所有订阅实例独立读取的广播事件"
            } else {
                "通过提交顺序维护广播范围的最新位置"
            })
            .col(
                ColumnDef::new(Alias::new("Id"))
                    .comment("主键标识ID")
                    .big_integer()
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(Alias::new("Scope"))
                    .comment(if name == "EventBusMessage" {
                        "所属广播范围"
                    } else {
                        "互通实例共用的广播范围"
                    })
                    .string_len(200)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("Position"))
                    .comment(if name == "EventBusMessage" {
                        "范围内遵循提交顺序的读取位置"
                    } else {
                        "当前已经提交的最大事件位置"
                    })
                    .big_integer()
                    .not_null(),
            );
        if name == "EventBusMessage" {
            table
                .col(
                    ColumnDef::new(Alias::new("EventId"))
                        .comment("广播事件标识")
                        .uuid()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Alias::new("EventName"))
                        .comment("共享事件类型的完整名称")
                        .string_len(1024)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Alias::new("Payload"))
                        .comment("使用统一约定序列化的事件参数")
                        .text()
                        .not_null(),
                );
        }
        table
            .col(
                ColumnDef::new(Alias::new("CreateTime"))
                    .comment("创建时间")
                    .timestamp_with_time_zone()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("DeleteTime"))
                    .comment("删除时间，不为 null 时表示已软删除")
                    .timestamp_with_time_zone()
                    .null(),
            );
        let mut table = TableModel::from_table(None, name, &table)?;
        table.add_audit_indexes()?;
        if name == "EventBusStream" {
            table.indexes.insert(
                "IX_EventBusStream_Scope".into(),
                r#"CREATE UNIQUE INDEX "IX_EventBusStream_Scope" ON "EventBusStream" ("Scope")"#
                    .into(),
            );
        } else {
            table.indexes.insert("IX_EventBusMessage_Scope_Position".into(), r#"CREATE UNIQUE INDEX "IX_EventBusMessage_Scope_Position" ON "EventBusMessage" ("Scope", "Position")"#.into());
            table.indexes.insert("IX_EventBusMessage_Scope_CreateTime".into(), r#"CREATE INDEX "IX_EventBusMessage_Scope_CreateTime" ON "EventBusMessage" ("Scope", "CreateTime")"#.into());
        }
        model.add_table(table)?;
    }
    Ok(model)
}

pub fn json_index_tables() -> Vec<repository_core::json_index::JsonIndexTable> {
    ["EventBusStream", "EventBusMessage"]
        .into_iter()
        .map(|name| repository_core::json_index::JsonIndexTable::without_indexes(None, name.into()))
        .collect()
}
/// 唯一约束包含软删除行；位置状态不得随历史清理删除。
pub fn initial_schema_statements(_: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("migrations/202611070001.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
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
