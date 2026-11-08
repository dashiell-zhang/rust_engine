use super::{ModelSnapshot, Operation, TableModel, literal, quote};
use std::collections::{BTreeMap, BTreeSet};

/// 仅比较两份声明，不连接数据库；交换参数即可生成结构回退。
pub fn changes(old: &ModelSnapshot, new: &ModelSnapshot) -> Result<Vec<Operation>, String> {
    let mut operations = Vec::new();
    for extension in new.extensions.difference(&old.extensions) {
        sql(
            &mut operations,
            format!("CREATE EXTENSION IF NOT EXISTS {}", quote(extension)),
        );
    }
    let schemas: BTreeSet<_> = new
        .tables
        .values()
        .filter_map(|table| table.schema.as_ref())
        .collect();
    for schema in schemas {
        if !old
            .tables
            .values()
            .any(|t| t.schema.as_ref() == Some(schema))
        {
            sql(
                &mut operations,
                format!("CREATE SCHEMA IF NOT EXISTS {}", quote(schema)),
            );
        }
    }
    for (key, table) in &new.tables {
        if let Some(partition) = &table.partition {
            partition.validate()?;
            if partition.schema != table.schema
                || partition.table != table.name
                || !table.columns.iter().any(|c| c.name == partition.key_column)
            {
                return Err(format!("{key} 的分区声明与表映射不一致"));
            }
        }
        if let Some(previous) = old.tables.get(key) {
            match (&previous.partition, &table.partition) {
                (None, None) => {}
                (Some(a), Some(b)) if a.key_column == b.key_column => {
                    let old_key = previous.columns.iter().find(|c| c.name == a.key_column);
                    let new_key = table.columns.iter().find(|c| c.name == b.key_column);
                    if old_key.map(|c| &c.sql_type) != new_key.map(|c| &c.sql_type) {
                        return Err(format!("{key} 不能通过普通迁移改变分区键类型"));
                    }
                }
                _ => {
                    return Err(format!(
                        "{key} 不能通过普通迁移新增、移除或改变分区键，请单独设计数据迁移"
                    ));
                }
            }
        }
    }
    let index_renames: BTreeMap<_, _> = old
        .tables
        .iter()
        .filter_map(|(key, table)| {
            let next = new.tables.get(key)?;
            (!rebuild_indexes(table, next)).then(|| {
                (
                    key,
                    renamed_definitions(&table.indexes, &next.indexes, index_definition),
                )
            })
        })
        .collect();
    let foreign_key_renames: BTreeMap<_, _> = old
        .tables
        .iter()
        .filter_map(|(key, table)| {
            let next = new.tables.get(key)?;
            Some((
                key,
                renamed_definitions(
                    &table.foreign_keys,
                    &next.foreign_keys,
                    |name, definition| {
                        definition
                            .strip_prefix(&format!(
                                "ALTER TABLE {key} ADD CONSTRAINT {} ",
                                quote(name)
                            ))
                            .map(str::to_owned)
                    },
                ),
            ))
        })
        .collect();
    // 删除/类型修改/主键调整前撤销关系，全部表完成后重建，支持循环外键。
    let rebuild_relations = old.tables.iter().any(|(key, table)| {
        new.tables.get(key).is_none_or(|next| {
            table.primary_key != next.primary_key
                || table.columns.iter().any(|column| {
                    next.columns
                        .iter()
                        .find(|c| c.name == column.name)
                        .is_none_or(|c| c.sql_type != column.sql_type || c.unique != column.unique)
                })
                || table.indexes.iter().any(|(name, sql)| {
                    next.indexes.get(name) != Some(sql)
                        && !is_non_unique_index(sql)
                        && !index_renames
                            .get(key)
                            .is_some_and(|renames| renames.contains_key(name))
                })
        })
    });
    for (key, table) in &old.tables {
        let next = new.tables.get(key);
        for (name, definition) in &table.foreign_keys {
            if !rebuild_relations
                && let Some(new_name) = foreign_key_renames
                    .get(key)
                    .and_then(|renames| renames.get(name))
            {
                sql(
                    &mut operations,
                    format!(
                        "ALTER TABLE {key} RENAME CONSTRAINT {} TO {}",
                        quote(name),
                        quote(new_name)
                    ),
                );
            } else if rebuild_relations
                || next.and_then(|t| t.foreign_keys.get(name)) != Some(definition)
            {
                sql(
                    &mut operations,
                    format!("ALTER TABLE {key} DROP CONSTRAINT {}", quote(name)),
                );
            }
        }
        if let Some(next) = next {
            for (name, definition) in &table.indexes {
                if let Some(new_name) = index_renames.get(key).and_then(|renames| renames.get(name))
                {
                    sql(
                        &mut operations,
                        format!(
                            "ALTER INDEX {} RENAME TO {}",
                            qualified(table.schema.as_deref(), name),
                            quote(new_name)
                        ),
                    );
                } else if rebuild_indexes(table, next) || next.indexes.get(name) != Some(definition)
                {
                    sql(
                        &mut operations,
                        format!("DROP INDEX {}", qualified(table.schema.as_deref(), name)),
                    );
                }
            }
        }
    }
    for key in old
        .tables
        .keys()
        .filter(|key| !new.tables.contains_key(*key))
    {
        sql(&mut operations, format!("DROP TABLE {key}"));
    }
    for (key, table) in &new.tables {
        if let Some(previous) = old.tables.get(key) {
            alter(&mut operations, previous, table)?;
        } else {
            sql(&mut operations, table.create_statement());
            if let Some(partition) = &table.partition {
                operations.push(Operation::InitialPartition(partition.clone()));
            }
        }
        let previous = old.tables.get(key);
        if previous.and_then(|t| t.comment.as_ref()) != table.comment.as_ref() {
            sql(
                &mut operations,
                format!("COMMENT ON TABLE {key} IS {}", comment(&table.comment)),
            );
        }
        for column in &table.columns {
            let old_comment = previous
                .and_then(|t| t.columns.iter().find(|c| c.name == column.name))
                .and_then(|c| c.comment.as_ref());
            if old_comment != column.comment.as_ref() {
                sql(
                    &mut operations,
                    format!(
                        "COMMENT ON COLUMN {key}.{} IS {}",
                        quote(&column.name),
                        comment(&column.comment)
                    ),
                );
            }
        }
        for (name, definition) in &table.indexes {
            if !index_renames
                .get(key)
                .is_some_and(|renames| renames.values().any(|new_name| new_name == name))
                && previous.is_none_or(|p| {
                    rebuild_indexes(p, table) || p.indexes.get(name) != Some(definition)
                })
            {
                sql(&mut operations, definition.clone());
            }
        }
    }
    for (key, table) in &new.tables {
        for (name, definition) in &table.foreign_keys {
            if rebuild_relations
                || (old.tables.get(key).and_then(|t| t.foreign_keys.get(name)) != Some(definition)
                    && !foreign_key_renames
                        .get(key)
                        .is_some_and(|renames| renames.values().any(|new_name| new_name == name)))
            {
                sql(&mut operations, definition.clone());
            }
        }
    }
    Ok(operations)
}

// 只识别声明生成的 SQL，要求除名称外完全相同且一一对应。
// 同名占用、重复定义或无法识别的 SQL 保留删除/重建行为，不猜测名称交换。
fn renamed_definitions(
    old: &BTreeMap<String, String>,
    new: &BTreeMap<String, String>,
    signature: impl Fn(&str, &str) -> Option<String>,
) -> BTreeMap<String, String> {
    let removed: Vec<_> = old
        .iter()
        .filter(|(name, _)| !new.contains_key(*name))
        .filter_map(|(name, definition)| {
            signature(name, definition).map(|signature| (name, signature))
        })
        .collect();
    let added: Vec<_> = new
        .iter()
        .filter(|(name, _)| !old.contains_key(*name))
        .filter_map(|(name, definition)| {
            signature(name, definition).map(|signature| (name, signature))
        })
        .collect();
    let mut renames = BTreeMap::new();
    for (name, signature) in &removed {
        if removed
            .iter()
            .filter(|(_, candidate)| candidate == signature)
            .count()
            != 1
        {
            continue;
        }
        let mut matches = added.iter().filter(|(_, candidate)| candidate == signature);
        if let Some((new_name, _)) = matches.next()
            && matches.next().is_none()
        {
            renames.insert((*name).clone(), (*new_name).clone());
        }
    }
    renames
}

fn index_definition(name: &str, definition: &str) -> Option<String> {
    for prefix in ["CREATE INDEX ", "CREATE UNIQUE INDEX "] {
        if let Some(rest) = definition.strip_prefix(&format!("{prefix}{}", quote(name)))
            && rest.chars().next().is_some_and(char::is_whitespace)
        {
            return Some(format!("{prefix}{rest}"));
        }
    }
    None
}

// 外键不能引用普通索引。只在可以明确识别时跳过关系重建；唯一索引及其他 SQL 保守处理。
fn is_non_unique_index(definition: &str) -> bool {
    let mut words = definition.split_whitespace();
    words
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("CREATE"))
        && words
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("INDEX"))
}

fn rebuild_indexes(old: &TableModel, new: &TableModel) -> bool {
    old.columns.iter().any(|column| {
        new.columns
            .iter()
            .find(|c| c.name == column.name)
            .is_none_or(|c| c.sql_type != column.sql_type)
    })
}

fn alter(
    operations: &mut Vec<Operation>,
    old: &TableModel,
    new: &TableModel,
) -> Result<(), String> {
    let table = new.qualified_name();
    if old.primary_key != new.primary_key && !old.primary_key.is_empty() {
        sql(
            operations,
            format!(
                "ALTER TABLE {table} DROP CONSTRAINT {}",
                quote(&old.primary_key_constraint_name())
            ),
        );
    } else if !old.primary_key.is_empty()
        && old.primary_key == new.primary_key
        && old.primary_key_constraint_name() != new.primary_key_constraint_name()
    {
        sql(
            operations,
            format!(
                "ALTER TABLE {table} RENAME CONSTRAINT {} TO {}",
                quote(&old.primary_key_constraint_name()),
                quote(&new.primary_key_constraint_name())
            ),
        );
    }
    for column in old
        .columns
        .iter()
        .filter(|c| !new.columns.iter().any(|n| n.name == c.name))
    {
        sql(
            operations,
            format!("ALTER TABLE {table} DROP COLUMN {}", quote(&column.name)),
        );
    }
    for column in &new.columns {
        let Some(previous) = old.columns.iter().find(|c| c.name == column.name) else {
            sql(
                operations,
                format!("ALTER TABLE {table} ADD COLUMN {}", column.definition()),
            );
            continue;
        };
        let name = quote(&column.name);
        let prefix = format!("ALTER TABLE {table} ALTER COLUMN {name}");
        if previous.auto_increment != column.auto_increment
            || (column.auto_increment && previous.sql_type != column.sql_type)
        {
            return Err(format!("{table}.{name} 的序列变化需要显式迁移"));
        }
        let type_changed = previous.sql_type != column.sql_type;
        if (type_changed || previous.default != column.default) && previous.default.is_some() {
            sql(operations, format!("{prefix} DROP DEFAULT"));
        }
        if type_changed {
            sql(operations, format!("{prefix} TYPE {}", column.sql_type));
        }
        if (type_changed || previous.default != column.default)
            && let Some(default) = &column.default
        {
            sql(operations, format!("{prefix} SET DEFAULT {default}"));
        }
        if previous.nullable != column.nullable {
            sql(
                operations,
                format!(
                    "{prefix} {} NOT NULL",
                    if column.nullable { "DROP" } else { "SET" }
                ),
            );
        }
        if previous.unique != column.unique {
            if column.unique {
                sql(
                    operations,
                    format!("ALTER TABLE {table} ADD UNIQUE ({name})"),
                );
            } else {
                sql(
                    operations,
                    format!(
                        "ALTER TABLE {table} DROP CONSTRAINT {}",
                        quote(&format!("{}_{}_key", new.name, column.name))
                    ),
                );
            }
        }
    }
    if old.primary_key != new.primary_key && !new.primary_key.is_empty() {
        sql(
            operations,
            format!("ALTER TABLE {table} ADD {}", new.primary_key_clause()),
        );
    }
    Ok(())
}

fn qualified(schema: Option<&str>, name: &str) -> String {
    schema
        .map(|s| format!("{}.{}", quote(s), quote(name)))
        .unwrap_or_else(|| quote(name))
}
fn comment(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(literal)
        .unwrap_or_else(|| "NULL".into())
}
fn sql(operations: &mut Vec<Operation>, value: String) {
    operations.push(Operation::Sql(value));
}
