use std::sync::Arc;

use chrono::{DateTime, FixedOffset};
use repository_core::{ReadDatabaseConnection, SoftDeleteEntityExt};
use sea_orm::{ColumnTrait, DbErr, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};

use crate::log;
use sea_orm::sea_query::{Expr, ExprTrait};

/// 日志组件自己的查询请求，不依赖应用 DTO 或业务仓库。
pub struct LogQueryRequest {
    pub skip: i32,
    pub take: i32,
    pub project: Option<String>,
    pub machine_name: Option<String>,
    pub content: Option<String>,
    pub level: Option<String>,
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
}

/// 查询契约不暴露持久化实体和系统行版本。
#[derive(sea_orm::FromQueryResult)]
pub struct LogQueryItem {
    #[sea_orm(from_alias = "Id")]
    pub id: i64,
    #[sea_orm(from_alias = "Project")]
    pub project: String,
    #[sea_orm(from_alias = "MachineName")]
    pub machine_name: String,
    #[sea_orm(from_alias = "Content")]
    pub content: String,
    #[sea_orm(from_alias = "Level")]
    pub level: String,
    #[sea_orm(from_alias = "Category")]
    pub category: String,
    #[sea_orm(from_alias = "CreateTime")]
    pub create_time: DateTime<FixedOffset>,
}

pub struct LogQueryResult {
    pub total: i32,
    pub list: Vec<LogQueryItem>,
}

/// 仅持有只读日志连接；创建查询服务不会启动写盘、ID 或后台任务。
pub struct LogQueryService {
    connection: Arc<ReadDatabaseConnection>,
}

impl LogQueryService {
    pub fn new(connection: Arc<ReadDatabaseConnection>) -> Self {
        Self { connection }
    }

    pub fn connection(&self) -> &ReadDatabaseConnection {
        &self.connection
    }

    pub async fn query(&self, request: LogQueryRequest) -> Result<LogQueryResult, DbErr> {
        if request.skip < 0 {
            return Err(DbErr::Custom("日志分页偏移不能为负数".into()));
        }
        if !(1..=100).contains(&request.take) {
            return Err(DbErr::Custom(
                "日志每页条数必须介于 1 和 100 之间".to_string(),
            ));
        }
        let mut query = log::Entity::find_active();
        for (column, value) in [
            (log::Column::Project, request.project),
            (log::Column::MachineName, request.machine_name),
            (log::Column::Content, request.content),
        ] {
            if let Some(value) = value.filter(|text| !text.trim().is_empty()) {
                let value = if matches!(column, log::Column::Content) {
                    value.trim()
                } else {
                    &value
                };
                // 对齐字符串 Contains：用户输入中的通配符按原文匹配。
                let pattern = repository_core::extensions::contains_pattern(value);
                query = query.filter(Expr::col(column).like(pattern));
            }
        }
        if let Some(level) = request.level.filter(|text| !text.trim().is_empty()) {
            query = query.filter(log::Column::Level.eq(level));
        }
        if let Some(time) = request.start_time {
            query = query.filter(log::Column::CreateTime.gte(time));
        }
        if let Some(time) = request.end_time {
            query = query.filter(log::Column::CreateTime.lte(time));
        }
        let total =
            repository_core::pagination::count_i32(query.clone().count(self.connection()).await?)?;
        let list = if total == 0 {
            Vec::new()
        } else {
            query
                .select_only()
                .columns([
                    log::Column::Id,
                    log::Column::Project,
                    log::Column::MachineName,
                    log::Column::Content,
                    log::Column::Level,
                    log::Column::Category,
                    log::Column::CreateTime,
                ])
                .order_by_desc(log::Column::Id)
                .offset(request.skip as u64)
                .limit(request.take as u64)
                .into_model::<LogQueryItem>()
                .all(self.connection())
                .await?
        };
        Ok(LogQueryResult { total, list })
    }
}
