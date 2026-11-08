//! 固定 UTC+8、单列 bigint 雪花 ID 的 PostgreSQL RANGE 分区。
mod maintenance;

pub use maintenance::{PartitionMaintenance, PartitionMaintenanceRuntime};

use chrono::{DateTime, Datelike, FixedOffset, Months, TimeDelta, TimeZone, Utc};
use identifier_generator::layout;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PartitionUnit {
    Hour,
    Day,
    Month,
    Year,
}

/// 模型声明必须同时用于迁移和运行时维护，不能通过数据库发现自动接管其他表。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PartitionTableDefinition {
    pub schema: Option<String>,
    pub table: String,
    pub key_column: String,
    pub interval: u32,
    pub unit: PartitionUnit,
}

/// 实体的分区声明，由所属模型的迁移和宿主注册使用。
pub trait PartitionedModel {
    fn partition_definition() -> PartitionTableDefinition;
}

/// 为声明了分区的 SeaORM 实体创建父表和当前唯一初始分区。
/// 主键、唯一约束与分区键的数据库约束由 PostgreSQL 在迁移事务内校验。
pub fn initial_schema_statements<E>(entity: E, now: DateTime<Utc>) -> Result<Vec<String>, String>
where
    E: sea_orm::EntityTrait,
    E::Model: PartitionedModel,
{
    let definition = E::Model::partition_definition();
    definition.validate()?;
    if entity.table_name() != definition.table
        || entity.schema_name() != definition.schema.as_deref()
    {
        return Err("分区声明与实体实际表映射不一致".into());
    }
    let range = definition.initial_range(now)?;
    let backend = sea_orm::DbBackend::Postgres;
    let mut table = crate::versioned::create_table_from_entity(entity, backend);
    table.extra(format!(
        "PARTITION BY RANGE ({})",
        quote_identifier(&definition.key_column)
    ));
    Ok(vec![
        backend.build(&table).to_string(),
        definition.create_child_statement(&range),
    ])
}

#[derive(Clone, Debug)]
pub struct PartitionRange {
    pub start_id: i64,
    pub end_id: i64,
    pub start_time: DateTime<FixedOffset>,
    pub end_time: DateTime<FixedOffset>,
}

#[derive(Clone, Debug)]
pub struct ExistingPartition {
    pub schema: String,
    pub name: String,
    pub start_id: i64,
    pub end_id: i64,
}

fn offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("固定 UTC+8")
}

pub(crate) fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 63 || value.contains('\0') {
        return Err(format!("PostgreSQL 标识符无效: {value:?}"));
    }
    Ok(())
}

impl PartitionTableDefinition {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier(&self.table)?;
        validate_identifier(&self.key_column)?;
        if let Some(schema) = &self.schema {
            validate_identifier(schema)?;
        }
        if self.interval == 0 || self.interval > i32::MAX as u32 {
            return Err("分区周期必须为正整数且不超过 i32 上限".into());
        }
        Ok(())
    }

    pub fn qualified_table(&self) -> String {
        qualified_name(self.schema.as_deref(), &self.table)
    }

    fn aligned_start(&self, time: DateTime<FixedOffset>) -> Result<DateTime<FixedOffset>, String> {
        let anchor = offset()
            .with_ymd_and_hms(1970, 1, 1, 0, 0, 0)
            .single()
            .expect("固定锚点");
        let interval = i64::from(self.interval);
        let result = match self.unit {
            PartitionUnit::Hour | PartitionUnit::Day => {
                let milliseconds = interval
                    * if self.unit == PartitionUnit::Hour {
                        3_600_000
                    } else {
                        86_400_000
                    };
                let elapsed = time.timestamp_millis() - anchor.timestamp_millis();
                anchor.checked_add_signed(TimeDelta::milliseconds(
                    elapsed / milliseconds * milliseconds,
                ))
            }
            PartitionUnit::Month => {
                let months = i64::from(time.year() - 1970) * 12 + i64::from(time.month0());
                let aligned = months / interval * interval;
                if aligned >= 0 {
                    u32::try_from(aligned)
                        .ok()
                        .and_then(|n| anchor.checked_add_months(Months::new(n)))
                } else {
                    u32::try_from(-aligned)
                        .ok()
                        .and_then(|n| anchor.checked_sub_months(Months::new(n)))
                }
            }
            PartitionUnit::Year => {
                let year = 1970 + i64::from(time.year() - 1970) / interval * interval;
                i32::try_from(year)
                    .ok()
                    .and_then(|y| offset().with_ymd_and_hms(y, 1, 1, 0, 0, 0).single())
            }
        };
        result.ok_or_else(|| "分区对齐时间超出范围".into())
    }

    fn add_interval(&self, time: DateTime<FixedOffset>) -> Result<DateTime<FixedOffset>, String> {
        let result = match self.unit {
            PartitionUnit::Hour => {
                time.checked_add_signed(TimeDelta::hours(i64::from(self.interval)))
            }
            PartitionUnit::Day => {
                time.checked_add_signed(TimeDelta::days(i64::from(self.interval)))
            }
            PartitionUnit::Month => time.checked_add_months(Months::new(self.interval)),
            PartitionUnit::Year => self
                .interval
                .checked_mul(12)
                .and_then(|n| time.checked_add_months(Months::new(n))),
        };
        result.ok_or_else(|| "分区结束时间超出范围".into())
    }

    pub fn initial_range(&self, now: DateTime<Utc>) -> Result<PartitionRange, String> {
        self.validate()?;
        if now.timestamp_millis() < layout::EPOCH_MILLISECONDS {
            return Err("当前时间早于雪花纪元，不能创建初始分区".into());
        }
        let start = self.aligned_start(now.with_timezone(&offset()))?;
        make_range(start, self.add_interval(start)?)
    }

    /// 政策变更只影响已有最右边界之后；允许首个新周期比完整周期短。
    pub fn next_range(&self, start_id: i64) -> Result<PartitionRange, String> {
        self.validate()?;
        if !layout::is_millisecond_boundary(start_id) {
            return Err("已有分区上界不是完整毫秒的最小雪花 ID".into());
        }
        let start = DateTime::from_timestamp_millis(layout::unix_millis_from_id(start_id))
            .ok_or("分区时间超出范围")?
            .with_timezone(&offset());
        make_range(start, self.add_interval(self.aligned_start(start)?)?)
    }

    /// 先规划全部范围，超过单轮 32 个的限制时不执行任何 DDL。
    pub fn required_ranges(
        &self,
        existing: &[ExistingPartition],
        now: DateTime<Utc>,
    ) -> Result<Vec<PartitionRange>, String> {
        self.validate()?;
        validate_ranges(existing)?;
        let current_id = layout::min_id_at_unix_millis(now.timestamp_millis())?;
        let mut partitions: Vec<_> = existing.iter().collect();
        partitions.sort_by_key(|p| p.start_id);
        let mut planned = Vec::new();
        let mut next_start = if let Some(current) = partitions
            .iter()
            .find(|p| p.start_id <= current_id && current_id < p.end_id)
        {
            current.end_id
        } else if let Some(last) = partitions.last() {
            if last.end_id > current_id {
                return Err("当前雪花 ID 未被覆盖，但存在更晚的分区；不自动填补历史空洞".into());
            }
            let mut end = last.end_id;
            while end <= current_id {
                let range = self.next_range(end)?;
                end = range.end_id;
                planned.push(range);
                ensure_create_limit(planned.len())?;
            }
            end
        } else {
            let current = self.initial_range(now)?;
            let end = current.end_id;
            planned.push(current);
            end
        };
        for _ in 0..3 {
            if let Some(existing) = partitions.iter().find(|p| p.start_id == next_start) {
                next_start = existing.end_id;
                continue;
            }
            if partitions.iter().any(|p| p.start_id > next_start) {
                return Err("当前分区之后存在范围空洞，停止预建".into());
            }
            let range = self.next_range(next_start)?;
            next_start = range.end_id;
            planned.push(range);
            ensure_create_limit(planned.len())?;
        }
        Ok(planned)
    }

    pub fn create_child_statement(&self, range: &PartitionRange) -> String {
        format!(
            "CREATE TABLE {} PARTITION OF {} FOR VALUES FROM ({}) TO ({})",
            qualified_name(
                self.schema.as_deref(),
                &partition_name(&self.table, range.start_time)
            ),
            self.qualified_table(),
            range.start_id,
            range.end_id
        )
    }
}

fn make_range(
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
) -> Result<PartitionRange, String> {
    let start_id = layout::min_id_at_unix_millis(start.timestamp_millis())?;
    let end_id = layout::min_id_at_unix_millis(end.timestamp_millis())?;
    if end_id <= start_id {
        return Err("分区范围无效".into());
    }
    Ok(PartitionRange {
        start_id,
        end_id,
        start_time: start,
        end_time: end,
    })
}

fn ensure_create_limit(count: usize) -> Result<(), String> {
    if count > 32 {
        Err("本轮需要创建超过 32 个分区，停止维护；请人工检查停机跨度与分区策略".into())
    } else {
        Ok(())
    }
}

pub fn validate_ranges(existing: &[ExistingPartition]) -> Result<(), String> {
    let mut sorted: Vec<_> = existing.iter().collect();
    sorted.sort_by_key(|p| p.start_id);
    let mut previous_end = None;
    for partition in sorted {
        if partition.start_id >= partition.end_id
            || !layout::is_millisecond_boundary(partition.start_id)
            || !layout::is_millisecond_boundary(partition.end_id)
            || previous_end.is_some_and(|end| partition.start_id < end)
        {
            return Err(format!(
                "子分区 {}.{} 边界无效或重叠",
                partition.schema, partition.name
            ));
        }
        previous_end = Some(partition.end_id);
    }
    Ok(())
}

pub fn partition_name(table: &str, start: DateTime<FixedOffset>) -> String {
    let mut suffix = format!("_p{}", start.with_timezone(&offset()).format("%Y%m%d%H"));
    if table.len() + suffix.len() <= 63 {
        return format!("{table}{suffix}");
    }
    let hash: String = Sha256::digest(table.as_bytes())[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    suffix.push('_');
    suffix.push_str(&hash[..8]);
    let mut end = 63 - suffix.len();
    while !table.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{suffix}", &table[..end])
}

fn qualified_name(schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(schema) => format!("{}.{}", quote_identifier(schema), quote_identifier(table)),
        None => quote_identifier(table),
    }
}
