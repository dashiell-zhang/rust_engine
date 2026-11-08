//! 离线关系模型快照。当前模型来自实体与所属组件声明，历史快照不再读取当前实体。
mod diff;
pub use diff::changes;

use crate::partition::PartitionTableDefinition;
use sea_orm::sea_query::{PostgresQueryBuilder, QueryBuilder, TableBuilder, TableCreateStatement};
use sea_orm::{DbBackend, EntityTrait, Schema};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSnapshot {
    pub tables: BTreeMap<String, TableModel>,
    pub extensions: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableModel {
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<ColumnModel>,
    pub primary_key: Vec<String>,
    /// 旧快照没有名称，继续使用 PostgreSQL 原有的 table_pkey 约定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_key_name: Option<String>,
    pub foreign_keys: BTreeMap<String, String>,
    pub indexes: BTreeMap<String, String>,
    pub partition: Option<PartitionTableDefinition>,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnModel {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub auto_increment: bool,
    pub unique: bool,
    pub comment: Option<String>,
}

/// 分区声明保持结构化，生成 SQL 时才计算初始范围。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
pub enum Operation {
    Sql(String),
    InitialPartition(PartitionTableDefinition),
}

pub fn statements(json: &str) -> Result<Vec<String>, String> {
    let operations: Vec<Operation> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let now = chrono::Utc::now();
    operations
        .into_iter()
        .map(|operation| match operation {
            Operation::Sql(sql) => Ok(sql),
            Operation::InitialPartition(definition) => {
                Ok(definition.create_child_statement(&definition.initial_range(now)?))
            }
        })
        .collect()
}

pub(super) fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(super) fn literal(value: &str) -> String {
    format!("E'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

impl ColumnModel {
    pub fn definition(&self) -> String {
        let sql_type = if self.auto_increment {
            match self.sql_type.as_str() {
                "smallint" => "smallserial",
                "integer" => "serial",
                "bigint" => "bigserial",
                value => value,
            }
        } else {
            &self.sql_type
        };
        let mut sql = format!("{} {sql_type}", quote(&self.name));
        if !self.nullable {
            sql.push_str(" NOT NULL");
        }
        if let Some(default) = &self.default {
            sql.push_str(&format!(" DEFAULT {default}"));
        }
        if self.unique {
            sql.push_str(" UNIQUE");
        }
        sql
    }
}

impl TableModel {
    pub fn from_entity<E: EntityTrait>(entity: E) -> Result<Self, String> {
        let backend = DbBackend::Postgres;
        if !Schema::new(backend)
            .create_enum_from_entity(entity)
            .is_empty()
        {
            return Err(format!(
                "表 {} 使用原生枚举；当前生成器尚未建模枚举类型迁移",
                entity.table_name()
            ));
        }
        let table = crate::versioned::create_table_from_entity(entity, backend);
        let mut model = Self::from_table(entity.schema_name(), entity.table_name(), &table)?;
        // 关系索引属于当前模型约定；一对一唯一性由所属模型显式覆盖。
        for foreign_key in table.get_foreign_key_create_stmts() {
            let columns = foreign_key.get_foreign_key().get_columns();
            model.add_index(&columns, false)?;
        }
        for index in Schema::new(backend).create_index_from_entity(entity) {
            let name = index
                .get_index_spec()
                .get_name()
                .ok_or("模型索引必须命名")?
                .to_owned();
            model
                .indexes
                .insert(name, backend.build(&index).to_string());
        }
        Ok(model)
    }

    /// 对应 CD/CUD 模型的时间索引；只为当前实体真实存在的列生成。
    pub fn add_audit_indexes(&mut self) -> Result<(), String> {
        for name in ["CreateTime", "DeleteTime", "UpdateTime"] {
            if self.columns.iter().any(|c| c.name == name) {
                self.add_index(&[name], false)?;
            }
        }
        Ok(())
    }

    pub fn add_index(&mut self, columns: &[impl AsRef<str>], unique: bool) -> Result<(), String> {
        if columns.is_empty()
            || columns
                .iter()
                .any(|c| !self.columns.iter().any(|column| column.name == c.as_ref()))
        {
            return Err(format!("表 {} 的索引包含未映射列", self.name));
        }
        let name = format!(
            "IX_{}_{}",
            self.name,
            columns
                .iter()
                .map(|c| c.as_ref())
                .collect::<Vec<_>>()
                .join("_")
        );
        let sql = format!(
            "CREATE {}INDEX {} ON {} ({})",
            if unique { "UNIQUE " } else { "" },
            quote(&name),
            self.qualified_name(),
            columns
                .iter()
                .map(|c| quote(c.as_ref()))
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.indexes.insert(name, sql);
        Ok(())
    }

    pub fn from_table(
        schema: Option<&str>,
        name: &str,
        table: &TableCreateStatement,
    ) -> Result<Self, String> {
        if table.get_extra().is_some() {
            return Err(format!("表 {name} 的额外 DDL 必须显式建模"));
        }
        let mut columns = Vec::new();
        let mut primary_key = Vec::new();
        let mut primary_key_name = None;
        for column in table.get_columns() {
            let column_name = column.get_column_name();
            if column_name == "xmin" {
                continue;
            }
            let spec = column.get_column_spec();
            if spec.check.is_some()
                || spec.generated.is_some()
                || spec.extra.is_some()
                || spec.using.is_some()
            {
                return Err(format!(
                    "{name}.{column_name} 包含尚未建模的列约束，请使用显式迁移"
                ));
            }
            let mut sql_type = String::new();
            if native_enum(column.get_column_type().ok_or("列缺少类型")?) {
                return Err(format!(
                    "{name}.{column_name} 使用尚未建模的原生枚举或枚举数组"
                ));
            }
            PostgresQueryBuilder
                .prepare_column_type(column.get_column_type().ok_or("列缺少类型")?, &mut sql_type);
            let default = spec.default.as_ref().map(|expr| {
                let mut sql = String::new();
                PostgresQueryBuilder.prepare_expr(expr, &mut sql);
                sql
            });
            if spec.primary_key {
                primary_key.push(column_name.clone());
            }
            columns.push(ColumnModel {
                name: column_name,
                sql_type,
                nullable: spec.nullable.unwrap_or(true) && !spec.primary_key,
                default,
                auto_increment: spec.auto_increment,
                unique: spec.unique,
                comment: spec.comment.clone(),
            });
        }
        for index in table.get_indexes() {
            if !index.is_primary_key() {
                return Err(format!("表 {name} 的复合约束应声明为命名索引"));
            }
            primary_key = index.get_index_spec().get_column_names();
            primary_key_name = index.get_index_spec().get_name().map(str::to_owned);
        }
        if !primary_key.is_empty()
            && primary_key_name
                .as_deref()
                .is_none_or(|key| key == format!("pk-{name}"))
        {
            primary_key_name = Some(format!("PK_{name}"));
        }
        for column in &mut columns {
            if primary_key.contains(&column.name) {
                column.nullable = false;
            }
        }
        let mut foreign_keys = BTreeMap::new();
        for fk in table.get_foreign_key_create_stmts() {
            let definition = fk.get_foreign_key();
            let original_name = definition.get_name().ok_or("外键必须命名")?;
            let columns = definition.get_columns();
            let constraint_name = if original_name == format!("fk-{name}-{}", columns.join("-")) {
                let Some(sea_orm::sea_query::TableRef::Table(referenced, _)) =
                    definition.get_ref_table()
                else {
                    return Err(format!("{name} 的外键引用必须为普通表"));
                };
                format!("FK_{name}_{}_{}", referenced.1, columns.join("_"))
            } else {
                original_name.to_owned()
            };
            let mut statement = fk.clone();
            statement.name(&constraint_name);
            if foreign_keys
                .insert(
                    constraint_name,
                    DbBackend::Postgres.build(&statement).to_string(),
                )
                .is_some()
            {
                return Err(format!("表 {name} 的外键名称重复"));
            }
        }
        Ok(Self {
            schema: schema.map(str::to_owned),
            name: name.to_owned(),
            columns,
            primary_key,
            primary_key_name,
            foreign_keys,
            indexes: BTreeMap::new(),
            partition: None,
            comment: table.get_comment().cloned(),
        })
    }

    pub fn qualified_name(&self) -> String {
        self.schema
            .as_ref()
            .map(|s| format!("{}.{}", quote(s), quote(&self.name)))
            .unwrap_or_else(|| quote(&self.name))
    }

    pub fn create_statement(&self) -> String {
        let mut parts = self
            .columns
            .iter()
            .map(ColumnModel::definition)
            .collect::<Vec<_>>();
        if !self.primary_key.is_empty() {
            parts.push(self.primary_key_clause());
        }
        let mut sql = format!(
            "CREATE TABLE {} ({})",
            self.qualified_name(),
            parts.join(", ")
        );
        if let Some(partition) = &self.partition {
            sql.push_str(&format!(
                " PARTITION BY RANGE ({})",
                quote(&partition.key_column)
            ));
        }
        sql
    }

    pub(super) fn primary_key_constraint_name(&self) -> String {
        self.primary_key_name
            .clone()
            .unwrap_or_else(|| format!("{}_pkey", self.name))
    }

    pub(super) fn primary_key_clause(&self) -> String {
        format!(
            "{}PRIMARY KEY ({})",
            self.primary_key_name
                .as_ref()
                .map(|name| format!("CONSTRAINT {} ", quote(name)))
                .unwrap_or_default(),
            self.primary_key
                .iter()
                .map(|column| quote(column))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn native_enum(column_type: &sea_orm::sea_query::ColumnType) -> bool {
    use sea_orm::sea_query::ColumnType;
    match column_type {
        ColumnType::Enum { .. } => true,
        ColumnType::Array(inner) => native_enum(inner),
        _ => false,
    }
}

impl ModelSnapshot {
    pub fn add_table(&mut self, table: TableModel) -> Result<(), String> {
        let key = table.qualified_name();
        if self.tables.insert(key.clone(), table).is_some() {
            return Err(format!("模型重复声明表 {key}"));
        }
        Ok(())
    }
}
