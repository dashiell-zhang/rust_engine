pub mod scaffold;
mod snapshots;

use crate::settings::DatabaseTarget;
use repository_core::migration::Migration;
use sea_orm::DatabaseConnection;

/// 返回全部迁移定义
fn all_migrations() -> &'static [Migration] {
    &[
        Migration {
            model_snapshot: None,
            version: 202604050001,
            build_down_statements: |backend| Ok(snapshots::initial_schema_down_statements(backend)),
            name: "initial_schema_code_first",
            build_statements: |backend| Ok(snapshots::initial_schema_statements(backend)),
        },
        Migration {
            model_snapshot: None,
            version: 202604050002,
            build_down_statements: |backend| {
                Ok(snapshots::legacy_alignment_down_statements(backend))
            },
            name: "align_legacy_schema_to_code_first",
            build_statements: |backend| Ok(snapshots::legacy_alignment_statements(backend)),
        },
        Migration {
            model_snapshot: None,
            version: 202606240001,
            build_down_statements: |backend| {
                Ok(snapshots::llm_model_alignment_down_statements(backend))
            },
            name: "align_llm_model_protocol_schema",
            build_statements: |backend| Ok(snapshots::llm_model_alignment_statements(backend)),
        },
        Migration {
            model_snapshot: None,
            version: 202611070001,
            build_down_statements: |backend| {
                Ok(snapshots::queue_component_alignment_down_statements(
                    backend,
                ))
            },
            name: "align_queue_component_boundaries",
            build_statements: |backend| {
                Ok(snapshots::queue_component_alignment_statements(backend))
            },
        },
        Migration {
            model_snapshot: None,
            version: 202611070002,
            build_down_statements: |backend| {
                Ok(snapshots::user_role_alignment_down_statements(backend))
            },
            name: "align_user_role_relationships",
            build_statements: |backend| Ok(snapshots::user_role_alignment_statements(backend)),
        },
        Migration {
            model_snapshot: None,
            version: 202611070003,
            build_down_statements: |backend| {
                Ok(snapshots::user_identity_alignment_down_statements(backend))
            },
            name: "align_user_identity_schema",
            build_statements: |backend| Ok(snapshots::user_identity_alignment_statements(backend)),
        },
        Migration {
            model_snapshot: None,
            version: 202611070004,
            build_down_statements: |backend| {
                Ok(snapshots::business_model_alignment_down_statements(backend))
            },
            name: "align_business_model_indexes",
            build_statements: |backend| Ok(snapshots::business_model_alignment_statements(backend)),
        },
    ]
}

/// 工具执行和离线导出共用同一个模型定义，避免迁移链发生分歧。
fn definition(target: DatabaseTarget) -> (&'static str, Vec<Migration>) {
    let (history, migrations): (&str, &[Migration]) = match target {
        DatabaseTarget::Business => (repository::MIGRATION_HISTORY_TABLE, all_migrations()),
        DatabaseTarget::EventBus => (
            event_bus_database::schema::MIGRATION_HISTORY_TABLE,
            &[Migration {
                model_snapshot: None,
                version: 202611070001,
                name: "initial_event_bus_component_schema",
                build_down_statements: |backend| {
                    Ok(event_bus_database::schema::initial_schema_down_statements(
                        backend,
                    ))
                },
                build_statements: |backend| {
                    Ok(event_bus_database::schema::initial_schema_statements(
                        backend,
                    ))
                },
            }],
        ),
        DatabaseTarget::Log => (
            logger_database::schema::MIGRATION_HISTORY_TABLE,
            &[Migration {
                model_snapshot: None,
                version: 202611070001,
                name: "initial_log_component_schema",
                build_down_statements: |backend| {
                    Ok(logger_database::schema::initial_schema_down_statements(
                        backend,
                    ))
                },
                build_statements: logger_database::schema::initial_schema_statements,
            }],
        ),
    };
    let mut migrations = migrations.to_vec();
    let generated: &[Migration] = match target {
        DatabaseTarget::Business => include!("generated/index.rs"),
        DatabaseTarget::Log => logger_database::schema::generated_migrations(),
        DatabaseTarget::EventBus => event_bus_database::schema::generated_migrations(),
    };
    migrations.extend_from_slice(generated);
    (history, migrations)
}

/// 每次只迁移所选写模型，历史表由所属模型声明。
pub async fn migrate(
    db: &DatabaseConnection,
    target: DatabaseTarget,
    to: Option<&str>,
) -> Result<(), String> {
    let (history_table, migrations) = definition(target);
    repository_core::migration::migrate(db, history_table, &migrations, to).await
}

/// 生成原生迁移链的 SQL，不访问数据库。
pub fn script(
    target: DatabaseTarget,
    options: repository_core::migration::ScriptOptions<'_>,
) -> Result<String, String> {
    let (history_table, migrations) = definition(target);
    repository_core::migration::create_script(history_table, &migrations, options)
}

/// 在连接数据库前校验迁移目标，防止将拼写错误变成数据库操作。
pub fn validate_target(target: DatabaseTarget, to: Option<&str>) -> Result<(), String> {
    let (_, migrations) = definition(target);
    repository_core::migration::resolve_target(&migrations, to)?;
    if to.is_none() && scaffold::has_pending_changes(target)? {
        return Err("当前模型存在尚未生成迁移的变化，请先生成、审查并重新编译迁移".into());
    }
    Ok(())
}
