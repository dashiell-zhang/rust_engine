//! JSON 索引模型与只读差异生成。声明归属实体模型，工具不能扫描并接管模型外的表。
use sea_orm::{
    ConnectionTrait, DbBackend, EntityTrait, Schema, Statement, TransactionSession,
    TransactionTrait,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, String>;

/// 声明由实体所属 crate 提供；需要索引时覆盖方法并补充属性映射和索引定义。
pub trait JsonIndexedEntity: EntityTrait {
    fn json_index_model() -> JsonIndexTable {
        JsonIndexTable::from_entity(Self::default())
    }
}

#[macro_export]
macro_rules! impl_json_index_model {
    ($($entity:ty),+ $(,)?) => { $(impl $crate::json_index::JsonIndexedEntity for $entity {})+ };
}

/// JSON 标量的持久化类型；日期、集合及任意对象不能作为标量索引。
#[derive(Clone, Copy, Debug)]
pub enum JsonScalarType {
    Text,
    SmallInt,
    Integer,
    BigInt,
    UnsignedBigInt,
    Decimal,
    Real,
    Double,
    Boolean,
    Uuid,
}
impl JsonScalarType {
    fn cast(self) -> Option<&'static str> {
        match self {
            Self::Text => None,
            Self::SmallInt => Some("smallint"),
            Self::Integer => Some("integer"),
            Self::BigInt => Some("bigint"),
            Self::UnsignedBigInt => Some("numeric(20,0)"),
            Self::Decimal => Some("numeric"),
            Self::Real => Some("real"),
            Self::Double => Some("double precision"),
            Self::Boolean => Some("boolean"),
            Self::Uuid => Some("uuid"),
        }
    }
}

/// 属性名用于索引声明，json_name 必须对应实际序列化名称。
#[derive(Clone, Debug)]
pub struct JsonMember {
    pub property: String,
    pub json_name: String,
    pub kind: JsonMemberKind,
}
#[derive(Clone, Debug)]
pub enum JsonMemberKind {
    Scalar(JsonScalarType),
    Object(Vec<JsonMember>),
    Collection,
    Unsupported,
}

/// 根属性映射到实际数据库列；复杂属性必须显式携带 JSON 成员元数据。
#[derive(Clone, Debug)]
pub struct IndexProperty {
    pub column: String,
    pub json: Option<JsonMemberKind>,
}

#[derive(Clone, Debug)]
pub struct JsonIndexField {
    pub property_path: String,
    pub descending: bool,
}
#[derive(Clone, Debug, Default)]
pub struct JsonIndexDefinition {
    pub fields: Vec<JsonIndexField>,
    pub unique: bool,
    pub all_descending: bool,
}

/// 每张表的名称和列类型从所属模型取得；声明不能依赖数据库扫描推断所有权。
#[derive(Clone, Debug)]
pub struct JsonIndexTable {
    pub schema: Option<String>,
    pub table: String,
    pub properties: BTreeMap<String, IndexProperty>,
    pub indexes: Vec<JsonIndexDefinition>,
    pub partition_key: Option<String>,
    columns: BTreeMap<String, bool>,
}
impl JsonIndexTable {
    pub fn from_entity<E: EntityTrait>(entity: E) -> Self {
        let table = Schema::new(DbBackend::Postgres).create_table_from_entity(entity);
        let columns = table
            .get_columns()
            .iter()
            .map(|column| {
                (
                    column.get_column_name(),
                    matches!(
                        column.get_column_type(),
                        Some(
                            sea_orm::sea_query::ColumnType::Json
                                | sea_orm::sea_query::ColumnType::JsonBinary
                        )
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();
        Self {
            schema: entity.schema_name().map(str::to_string),
            table: entity.table_name().to_string(),
            properties: columns
                .keys()
                .map(|name| {
                    (
                        name.clone(),
                        IndexProperty {
                            column: name.clone(),
                            json: None,
                        },
                    )
                })
                .collect(),
            columns,
            indexes: Vec::new(),
            partition_key: None,
        }
    }

    /// 原生 SQL 模型没有 SeaORM 实体时，所属组件显式提供表名。
    /// 没有列元数据的表只能参与既有工具索引清理，不能声明新索引。
    pub fn without_indexes(schema: Option<String>, table: String) -> Self {
        Self {
            schema,
            table,
            properties: BTreeMap::new(),
            columns: BTreeMap::new(),
            indexes: Vec::new(),
            partition_key: None,
        }
    }

    fn expression(&self, path: &str) -> Result<String> {
        let parts = path.split('.').collect::<Vec<_>>();
        if parts.iter().any(|part| part.is_empty()) {
            return Err(format!("表 {} 的索引属性路径为空：{path}", self.table));
        }
        let property = self
            .properties
            .get(parts[0])
            .ok_or_else(|| format!("表 {} 未映射属性 {path}", self.table))?;
        let is_json = self
            .columns
            .get(&property.column)
            .ok_or_else(|| format!("属性 {path} 未映射到表 {} 的实际列", self.table))?;
        let mut expression = quote(&property.column)?;
        if parts.len() == 1 {
            if *is_json || property.json.is_some() {
                return Err(format!("属性 {path} 是 JSON 复杂属性，需要指定标量路径"));
            }
            return Ok(expression);
        }
        if !is_json {
            return Err(format!("属性 {path} 的根列不是 JSON 列"));
        }
        let mut current = property
            .json
            .as_ref()
            .ok_or_else(|| format!("属性 {path} 没有 JSON 映射"))?;
        for (index, part) in parts.iter().enumerate().skip(1) {
            let members = match current {
                JsonMemberKind::Object(members) => members,
                JsonMemberKind::Collection => {
                    return Err(format!("JSON 索引不支持集合路径：{path}"));
                }
                _ => return Err(format!("JSON 索引路径 {path} 不能继续访问属性")),
            };
            let mut candidates = members.iter().filter(|member| member.property == *part);
            let member = candidates
                .next()
                .ok_or_else(|| format!("JSON 路径 {path} 不存在"))?;
            if candidates.next().is_some() {
                return Err(format!("JSON 路径 {path} 存在重复属性映射"));
            }
            let name = literal(&member.json_name)?;
            if index + 1 == parts.len() {
                let JsonMemberKind::Scalar(kind) = member.kind else {
                    return Err(format!("JSON 索引仅支持标量路径，不支持集合或对象：{path}"));
                };
                expression = format!("({expression} ->> {name}::text)");
                return Ok(match kind.cast() {
                    Some(cast) => format!("(({expression})::{cast})"),
                    None => format!("({expression})"),
                });
            }
            expression = format!("({expression} -> {name}::text)");
            current = &member.kind;
        }
        Err(format!("无效的 JSON 索引路径：{path}"))
    }

    fn expected(&self, schema: &str) -> Result<BTreeMap<String, String>> {
        let mut expected = BTreeMap::new();
        for declaration in &self.indexes {
            if declaration.fields.is_empty() {
                return Err(format!("表 {} 的 JSON 索引没有字段", self.table));
            }
            let mut paths = BTreeSet::new();
            let mut expressions = Vec::new();
            for field in &declaration.fields {
                if !paths.insert(field.property_path.as_str()) {
                    return Err(format!("JSON 索引字段重复：{}", field.property_path));
                }
                let mut expression = self.expression(&field.property_path)?;
                if declaration.all_descending || field.descending {
                    expression.push_str(" DESC");
                }
                expressions.push(expression);
            }
            if declaration.unique
                && let Some(key) = &self.partition_key
                && !declaration.fields.iter().any(|field| {
                    !field.property_path.contains('.')
                        && self
                            .properties
                            .get(&field.property_path)
                            .is_some_and(|property| {
                                property.json.is_none() && property.column == *key
                            })
                })
            {
                return Err(format!(
                    "分区表 {} 的唯一 JSON 索引必须包含分区键 {key}",
                    self.table
                ));
            }
            let unique = if declaration.unique { " UNIQUE" } else { "" };
            let target = format!("{}.{}", quote(schema)?, quote(&self.table)?);
            let expressions = expressions.join(", ");
            let definition = format!("{unique} ON {target} USING btree ({expressions})");
            let hash = Sha256::digest(definition.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let name = format!("jsonbIX_{}", &hash[..40]);
            let sql = format!(
                "CREATE{unique} INDEX {} ON {target} USING btree ({expressions});",
                quote(&name)?
            );
            expected.insert(name, sql);
        }
        Ok(expected)
    }
}

/// 只读事务固定连接及目录快照，只返回 SQL，不执行任何生成语句。
pub async fn create_script<C: TransactionTrait>(
    db: &C,
    tables: &[JsonIndexTable],
) -> Result<String> {
    let transaction = db.begin().await.map_err(|error| error.to_string())?;
    transaction
        .execute_unprepared("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await
        .map_err(|error| error.to_string())?;
    let mut expected_tables = BTreeMap::<(String, String), BTreeMap<String, String>>::new();
    for table in tables {
        let name = match &table.schema {
            Some(schema) => format!("{}.{}", quote(schema)?, quote(&table.table)?),
            None => quote(&table.table)?,
        };
        let row = transaction.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "SELECT n.nspname FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE c.oid = pg_catalog.to_regclass($1)",
            [name.clone().into()])).await.map_err(|error| error.to_string())?
            .ok_or_else(|| format!("所选模型的表 {name} 不存在"))?;
        let schema: String = row
            .try_get("", "nspname")
            .map_err(|error| error.to_string())?;
        expected_tables
            .entry((schema.clone(), table.table.clone()))
            .or_default()
            .extend(table.expected(&schema)?);
    }
    let mut result = String::new();
    for ((schema, table), expected) in expected_tables {
        let rows = transaction.query_all_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "SELECT indexname FROM pg_catalog.pg_indexes WHERE schemaname = $1 AND tablename = $2",
            [schema.clone().into(), table.into()])).await.map_err(|error| error.to_string())?;
        let mut existing = BTreeSet::new();
        for row in rows {
            let name: String = row
                .try_get("", "indexname")
                .map_err(|error| error.to_string())?;
            if is_managed_name(&name) {
                existing.insert(name);
            }
        }
        for name in existing.iter().filter(|name| !expected.contains_key(*name)) {
            result.push_str(&format!(
                "DROP INDEX {}.{};\n",
                quote(&schema)?,
                quote(name)?
            ));
        }
        for (name, sql) in expected {
            if !existing.contains(&name) {
                result.push_str(&sql);
                result.push('\n');
            }
        }
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(result)
}

fn is_managed_name(name: &str) -> bool {
    name.strip_prefix("jsonbIX_").is_some_and(|hash| {
        (32..=48).contains(&hash.len()) && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}
fn quote(value: &str) -> Result<String> {
    if value.is_empty() || value.len() > 63 || value.contains('\0') {
        return Err("数据库标识符必须为 1～63 个有效 UTF-8 字节".into());
    }
    Ok(format!("\"{}\"", value.replace('"', "\"\"")))
}
fn literal(value: &str) -> Result<String> {
    if value.contains('\0') {
        return Err("JSON 属性名称不能包含 NUL".into());
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}
