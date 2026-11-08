use super::models::{
    TaskExecutionLogDto, TaskExecutionLogPageListDto, TaskExecutionLogPageRequest,
};
use crate::user::models::PageListDto;
use crate::write_support::{page_offset, validate_page_request};
use common::app_error::AppError;
use logger_database::{log, query::LogQueryService};
use repository::extensions::SoftDeleteQueryExt;
use repository_core::pagination::SignedPage;
use sea_orm::{ColumnTrait, EntityTrait, ExprTrait, PaginatorTrait, QueryFilter, QueryOrder};

/// 任务执行日志查询独立于业务任务表，始终读取日志组件的只读连接。
pub struct TaskExecutionLogService<'a> {
    query: &'a LogQueryService,
}
impl<'a> TaskExecutionLogService<'a> {
    pub fn new(query: &'a LogQueryService) -> Self {
        Self { query }
    }
    /// 获取任务执行日志列表
    pub async fn get_task_execution_log_list(
        &self,
        params: TaskExecutionLogPageRequest,
    ) -> Result<TaskExecutionLogPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query = log::Entity::find()
            .not_deleted(log::Column::DeleteTime)
            .filter(
                log::Column::Category
                    .eq("task_service.queue.execution_result")
                    .or(log::Column::Category.eq("task_service.schedule.execution_result")),
            );

        if let Some(task_id) = params.task_id {
            if task_id <= 0 {
                return Err(AppError::bad_request("taskId 必须大于 0"));
            }

            query = query.filter(log::Column::Content.contains(format!("\"task_id\":{task_id},")));
        }

        if let Some(task_name) = params
            .task_name
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let task_name_json = serde_json::to_string(task_name)
                .map_err(|error| AppError::internal(format!("taskName 序列化失败: {error}")))?;
            query = query
                .filter(log::Column::Content.contains(format!("\"task_name\":{task_name_json}")));
        }

        if let Some(result) = params
            .result
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let result_json = serde_json::to_string(result)
                .map_err(|error| AppError::internal(format!("result 序列化失败: {error}")))?;
            query =
                query.filter(log::Column::Content.contains(format!("\"result\":{result_json}")));
        }

        if let Some(level) = params
            .level
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            query = query.filter(log::Column::Level.eq(level));
        }

        if let Some(task_type) = params
            .task_type
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            query =
                match task_type {
                    "queue" => query
                        .filter(log::Column::Category.eq("task_service.queue.execution_result")),
                    "schedule" => query
                        .filter(log::Column::Category.eq("task_service.schedule.execution_result")),
                    _ => return Err(AppError::bad_request("taskType 仅支持 queue 或 schedule")),
                };
        }

        if let Some(start_create_time) = params.start_create_time {
            query = query.filter(log::Column::CreateTime.gte(start_create_time));
        }

        if let Some(end_create_time) = params.end_create_time {
            query = query.filter(log::Column::CreateTime.lte(end_create_time));
        }

        let total = query
            .clone()
            .count(self.query.connection())
            .await
            .map_err(to_app_error)?;
        let total = repository_core::pagination::count_i32(total).map_err(to_app_error)?;

        if total == 0 {
            return Ok(PageListDto {
                total: 0,
                list: Some(vec![]),
            });
        }

        let skip = page_offset(params.page, params.page_size);
        let list = query
            .order_by_desc(log::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.query.connection())
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(build_task_execution_log_dto)
            .collect();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }
}

/// 组装任务执行日志返回模型
fn build_task_execution_log_dto(model: log::Model) -> TaskExecutionLogDto {
    let content_json = serde_json::from_str::<serde_json::Value>(&model.content).ok();
    let task_id = content_json
        .as_ref()
        .and_then(|value| value.get("task_id"))
        .and_then(|value| value.as_i64());
    let task_name = content_json
        .as_ref()
        .and_then(|value| value.get("task_name"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let result = content_json
        .as_ref()
        .and_then(|value| value.get("result"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let error = content_json
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(|value| value.as_str())
        .map(str::to_string);

    TaskExecutionLogDto {
        id: model.id.to_string(),
        project: model.project,
        machine_name: model.machine_name,
        level: model.level,
        category: model.category,
        task_id,
        task_name,
        result,
        error,
        content: model.content,
        content_json,
        create_time: model.create_time.to_rfc3339(),
    }
}

fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}
