use repository::extensions::SoftDeleteQueryExt;
use repository::log;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};

use application_interface::context::AppContext;
use common::app_error::AppError;

use crate::user::models::PageListDto;
use crate::write_support::{normalize_page, validate_page_request};

use super::models::{LogDto, LogPageListDto, LogPageRequest};

/// 日志管理应用服务
pub struct LogManageService<'a> {
    /// 应用上下文
    context: AppContext<'a>,
}

impl<'a> LogManageService<'a> {
    /// 创建日志管理应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 获取日志分页列表
    pub async fn get_log_list(&self, params: LogPageRequest) -> Result<LogPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query = log::Entity::find().not_deleted(log::Column::DeleteTime);

        if let Some(project) = params
            .project
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            query = query.filter(log::Column::Project.contains(project));
        }

        if let Some(machine_name) = params
            .machine_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            query = query.filter(log::Column::MachineName.contains(machine_name));
        }

        if let Some(level) = params
            .level
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            query = query.filter(log::Column::Level.eq(level));
        }

        if let Some(content) = params
            .content
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            query = query.filter(log::Column::Content.contains(content));
        }

        if let Some(start_time) = params.start_time {
            query = query.filter(log::Column::CreateTime.gte(start_time));
        }

        if let Some(end_time) = params.end_time {
            query = query.filter(log::Column::CreateTime.lte(end_time));
        }

        let total = query
            .clone()
            .count(self.context.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        if total == 0 {
            return Ok(PageListDto {
                total: 0,
                list: vec![],
            });
        }

        let page = normalize_page(params.page);
        let skip = (page - 1) * params.page_size;

        let list = query
            .order_by_desc(log::Column::Id)
            .offset(skip)
            .limit(params.page_size)
            .all(self.context.db)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?
            .into_iter()
            .map(|m| LogDto {
                id: m.id.to_string(),
                project: m.project,
                machine_name: m.machine_name,
                level: m.level,
                category: m.category,
                content: m.content,
                create_time: m.create_time.to_rfc3339(),
            })
            .collect();

        Ok(PageListDto { total, list })
    }
}
