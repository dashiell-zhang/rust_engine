use super::models::{
    CreateQueueTaskDto, CreateScheduleTaskDto, EditTaskSettingDto, QueueTaskDto,
    QueueTaskPageListDto, QueueTaskPageRequest, TaskSettingDto, TaskSettingPageListDto,
    TaskSettingPageRequest,
};
use crate::user::models::PageListDto;
use crate::write_support::{
    begin_transaction, normalize_optional_text, page_offset, validate_page_request,
    write_data_update_log_with_connection,
};
use application_interface::{
    context::{AppContext, CurrentUser},
    error::AppError,
};
use chrono::Utc;
use queue_task_database::{
    NewQueueTask, NewScheduleTask, QueueTaskManagement, QueueTaskProducer, QueueTaskQuery,
    TaskSettingUpdate, queue_task, queue_task::QueueTaskStatus, task_setting,
};
use sea_orm::DatabaseTransaction;
use serde_json::{Value, json};

/// 任务中心只负责业务 DTO 和审计装配，管理规则由通用任务组件实现。
pub struct TaskCenterService<'a> {
    context: AppContext<'a>,
}
impl<'a> TaskCenterService<'a> {
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }
    fn management(&self) -> QueueTaskManagement<'_> {
        QueueTaskManagement::new(self.context.db, self.context.id_generator)
    }

    pub async fn get_task_setting_list(
        &self,
        params: TaskSettingPageRequest,
    ) -> Result<TaskSettingPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;
        let skip = page_offset(params.page, params.page_size);
        let result = self
            .management()
            .query_settings(skip, params.page_size, params.category.as_deref())
            .await?;
        Ok(PageListDto {
            total: result.total,
            list: Some(
                result
                    .list
                    .into_iter()
                    .map(build_task_setting_dto)
                    .collect(),
            ),
        })
    }

    pub async fn update_task_setting(
        &self,
        id: i64,
        dto: EditTaskSettingDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let tx = begin_transaction(self.context.db).await?;
        let change = self
            .management()
            .update_setting_in_transaction(
                &tx,
                id,
                current_user.user_id,
                TaskSettingUpdate {
                    parameter: dto.parameter,
                    semaphore: dto.semaphore,
                    cron: dto.cron,
                    is_enable: dto.is_enable,
                    remark: dto.remark,
                },
            )
            .await?;
        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "TaskSetting",
            id,
            "update",
            Some(setting_snapshot(&change.before)),
            Some(setting_snapshot(&change.after)),
            Some(current_user),
        )
        .await?;
        tx.commit().await.map_err(to_app_error)?;
        Ok(true)
    }

    pub async fn get_args_schedule_task_name_list(&self) -> Result<Vec<String>, AppError> {
        self.management().args_schedule_names().await
    }

    pub async fn create_schedule_task(
        &self,
        dto: CreateScheduleTaskDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        let tx = begin_transaction(self.context.db).await?;
        let model = self
            .management()
            .create_schedule_in_transaction(
                &tx,
                current_user.user_id,
                NewScheduleTask {
                    name: dto.name,
                    parameter: dto.parameter,
                    cron: dto.cron,
                    is_enable: dto.is_enable,
                    remark: dto.remark,
                },
            )
            .await?;
        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "TaskSetting",
            model.id,
            "create",
            None,
            Some(setting_snapshot(&model)),
            Some(current_user),
        )
        .await?;
        tx.commit().await.map_err(to_app_error)?;
        Ok(model.id.to_string())
    }

    pub async fn delete_task_setting(
        &self,
        id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let tx = begin_transaction(self.context.db).await?;
        let change = self
            .management()
            .delete_setting_in_transaction(&tx, id, current_user.user_id)
            .await?;
        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "TaskSetting",
            id,
            "delete",
            Some(setting_snapshot(&change.before)),
            Some(json!({ "id": id, "delete_user_id": current_user.user_id })),
            Some(current_user),
        )
        .await?;
        tx.commit().await.map_err(to_app_error)?;
        Ok(true)
    }

    pub async fn get_queue_task_list(
        &self,
        params: QueueTaskPageRequest,
    ) -> Result<QueueTaskPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;
        let skip = page_offset(params.page, params.page_size);
        let result = self
            .management()
            .query_tasks(QueueTaskQuery {
                skip,
                take: params.page_size,
                is_success: params.is_success,
                start: params.start_create_time,
                end: params.end_create_time,
                parameter_keyword: params.parameter_keyword,
            })
            .await?;
        Ok(PageListDto {
            total: result.total,
            list: Some(result.list.into_iter().map(build_queue_task_dto).collect()),
        })
    }

    /// 在显式事务中入队，调用方负责业务操作和任务一并提交。
    pub async fn create_queue_task(
        &self,
        tx: &DatabaseTransaction,
        dto: CreateQueueTaskDto,
    ) -> Result<String, AppError> {
        validate_create_queue_task(&dto)?;
        QueueTaskProducer::new(self.context.db, self.context.id_generator)
            .create(tx, queue_request(dto))
            .await
            .map(|id| id.to_string())
            .map_err(to_app_error)
    }

    pub async fn create_queue_task_single(
        &self,
        dto: CreateQueueTaskDto,
    ) -> Result<String, AppError> {
        validate_create_queue_task(&dto)?;
        QueueTaskProducer::new(self.context.db, self.context.id_generator)
            .create_single(queue_request(dto))
            .await
            .map(|id| id.to_string())
            .map_err(to_app_error)
    }

    pub async fn retry_queue_task(&self, id: i64) -> Result<bool, AppError> {
        self.management().retry(id).await?;
        Ok(true)
    }
}

fn setting_snapshot(model: &task_setting::Model) -> Value {
    json!({ "id": model.id, "name": model.name, "category": model.category, "semaphore": model.semaphore,
        "cron": model.cron, "parameter": model.parameter, "is_enable": model.is_enable, "remark": model.remark })
}

/// 校验创建队列任务请求
fn validate_create_queue_task(dto: &CreateQueueTaskDto) -> Result<(), AppError> {
    if dto.name.trim().is_empty() {
        return Err(AppError::bad_request("name 不可以空"));
    }

    if let Some(parameter) = dto
        .parameter
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        parse_json_text(parameter, "parameter 必须是合法 JSON")?;
    }

    if let Some(callback_parameter) = dto
        .callback_parameter
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if dto
            .callback_name
            .as_deref()
            .map(str::trim)
            .is_none_or(|item| item.is_empty())
        {
            return Err(AppError::bad_request("callbackName 不可以空"));
        }

        parse_json_text(callback_parameter, "callbackParameter 必须是合法 JSON")?;
    }

    if let Some(plan_time) = dto.plan_time
        && plan_time <= Utc::now().fixed_offset()
    {
        return Err(AppError::bad_request("planTime 必须大于当前时间"));
    }

    if let Some(parent_task_id) = dto.parent_task_id
        && parent_task_id <= 0
    {
        return Err(AppError::bad_request("parentTaskId 必须大于 0"));
    }

    Ok(())
}

/// 解析 JSON 文本
fn parse_json_text(value: &str, error_message: &str) -> Result<(), AppError> {
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|_| AppError::bad_request(error_message))
}

/// 组装任务配置返回模型
fn build_task_setting_dto(model: task_setting::Model) -> TaskSettingDto {
    TaskSettingDto {
        id: model.id.to_string(),
        name: model.name,
        category: model.category,
        semaphore: model.semaphore,
        cron: model.cron,
        parameter: model.parameter,
        is_enable: model.is_enable,
        remark: model.remark,
        create_time: model.create_time.to_rfc3339(),
        update_time: model.update_time.map(|item| item.to_rfc3339()),
    }
}

/// 组装队列任务返回模型
fn build_queue_task_dto(model: queue_task::Model) -> QueueTaskDto {
    QueueTaskDto {
        id: model.id.to_string(),
        name: model.name,
        parameter: model.parameter,
        status: queue_task_status_to_int(model.status),
        count: model.count,
        first_time: model.first_time.map(|item| item.to_rfc3339()),
        last_time: model.last_time.map(|item| item.to_rfc3339()),
        success_time: model.success_time.map(|item| item.to_rfc3339()),
        plan_time: model.plan_time.map(|item| item.to_rfc3339()),
        worker_id: model.worker_id,
        lease_expire_time: model.lease_expire_time.map(|item| item.to_rfc3339()),
        callback_name: model.callback_name,
        callback_parameter: model.callback_parameter,
        parent_task_id: model.parent_task_id.map(|item| item.to_string()),
        child_success_time: model.child_success_time.map(|item| item.to_rfc3339()),
        create_time: model.create_time.to_rfc3339(),
    }
}

/// 转换队列任务状态文本
fn queue_task_status_to_int(status: QueueTaskStatus) -> i32 {
    match status {
        QueueTaskStatus::Pending => 0,
        QueueTaskStatus::Running => 1,
        QueueTaskStatus::Succeeded => 2,
        QueueTaskStatus::Failed => 3,
    }
}

/// 转换数据库异常
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}

/// 应用 DTO 转换为通用任务组件的入队请求。
fn queue_request(dto: CreateQueueTaskDto) -> NewQueueTask {
    NewQueueTask {
        name: dto.name.trim().to_string(),
        parameter: normalize_optional_text(dto.parameter),
        plan_time: dto.plan_time,
        callback_name: normalize_optional_text(dto.callback_name),
        callback_parameter: normalize_optional_text(dto.callback_parameter),
        parent_task_id: dto.parent_task_id,
    }
}
