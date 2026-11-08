use common::app_error::AppError;
use logger_database::query::{LogQueryRequest, LogQueryService};

use super::models::{LogDto, LogPageListDto, LogPageRequest};
use crate::user::models::PageListDto;
use crate::write_support::{page_offset, validate_page_request};

/// 日志管理应用服务，只负责 DTO 转换，由日志组件执行查询。
pub struct LogManageService<'a> {
    query: &'a LogQueryService,
}

impl<'a> LogManageService<'a> {
    pub fn new(query: &'a LogQueryService) -> Self {
        Self { query }
    }

    pub async fn get_log_list(&self, params: LogPageRequest) -> Result<LogPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;
        let skip = page_offset(params.page, params.page_size);
        let result = self
            .query
            .query(LogQueryRequest {
                skip,
                take: params.page_size,
                project: params.project,
                machine_name: params.machine_name,
                content: params.content,
                level: params.level,
                start_time: params.start_time,
                end_time: params.end_time,
            })
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;
        let list = result
            .list
            .into_iter()
            .map(|model| LogDto {
                id: model.id.to_string(),
                project: model.project,
                machine_name: model.machine_name,
                level: model.level,
                category: model.category,
                content: model.content,
                create_time: model.create_time.to_rfc3339(),
            })
            .collect();
        Ok(PageListDto {
            total: result.total,
            list: Some(list),
        })
    }
}
