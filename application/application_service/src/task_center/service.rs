#![allow(clippy::collapsible_if, clippy::useless_conversion)]

use application_model::task_center::ARGS_DEFAULT_PARAMETER;
use chrono::Utc;
use repository::extensions::SoftDeleteQueryExt;
use repository::{log, queue_task, queue_task::QueueTaskStatus, task_setting};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseTransaction,
    EntityTrait, ExprTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde_json::json;

use application_interface::{
    context::{AppContext, CurrentUser},
    error::AppError,
};

use crate::user::models::PageListDto;
use crate::write_support::{
    apply_soft_delete_audit_fields, apply_update_user_audit_field, begin_transaction,
    build_create_full_audit_fields, normalize_optional_text, normalize_page, validate_page_request,
    write_data_update_log_with_connection,
};

use super::models::{
    CreateQueueTaskDto, CreateScheduleTaskDto, EditTaskSettingDto, QueueTaskDto,
    QueueTaskPageListDto, QueueTaskPageRequest, TaskExecutionLogDto, TaskExecutionLogPageListDto,
    TaskExecutionLogPageRequest, TaskSettingDto, TaskSettingPageListDto, TaskSettingPageRequest,
};

/// 任务中心应用服务
pub struct TaskCenterService<'a> {
    /// 任务中心上下文
    context: AppContext<'a>,
}

impl<'a> TaskCenterService<'a> {
    /// 创建任务中心应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 获取任务配置列表
    pub async fn get_task_setting_list(
        &self,
        params: TaskSettingPageRequest,
    ) -> Result<TaskSettingPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query = task_setting::Entity::find().not_deleted(task_setting::Column::DeleteTime);

        if let Some(category) = params
            .category
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            query = query.filter(task_setting::Column::Category.eq(category));
        }

        let total = query
            .clone()
            .count(self.context.db)
            .await
            .map_err(to_app_error)?;

        if total == 0 {
            return Ok(PageListDto {
                total: 0,
                list: vec![],
            });
        }

        let page = normalize_page(params.page);
        let skip = (page - 1) * params.page_size;
        let list = query
            .order_by_desc(task_setting::Column::Id)
            .offset(skip)
            .limit(params.page_size)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(build_task_setting_dto)
            .collect();

        Ok(PageListDto { total, list })
    }

    /// 更新任务配置信息
    pub async fn update_task_setting(
        &self,
        task_setting_id: i64,
        dto: EditTaskSettingDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        validate_task_setting_id(task_setting_id)?;

        let task_model = task_setting::Entity::find_by_id(task_setting_id)
            .not_deleted(task_setting::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(task_model) = task_model else {
            return Err(AppError::bad_request("无效的 taskSettingId"));
        };

        let is_args_template = is_args_schedule_task_template(&task_model);
        validate_edit_task_setting(&dto, is_args_template)?;

        if is_args_template
            && dto
                .parameter
                .as_deref()
                .map(str::trim)
                .is_some_and(|item| !item.is_empty() && item != ARGS_DEFAULT_PARAMETER)
        {
            return Err(AppError::bad_request(
                "带参定时任务模板不允许修改 parameter",
            ));
        }

        let before_log = json!({
            "id": task_model.id,
            "name": task_model.name,
            "category": task_model.category,
            "semaphore": task_model.semaphore,
            "cron": task_model.cron,
            "parameter": task_model.parameter,
            "is_enable": task_model.is_enable,
            "remark": task_model.remark,
        });
        let template_parameter = task_model.parameter.clone();
        let tx = begin_transaction(self.context.db).await?;
        let mut active = task_model.into_active_model();
        active.parameter = Set(if is_args_template {
            template_parameter
        } else {
            normalize_optional_text(dto.parameter)
        });
        active.semaphore = Set(dto.semaphore);
        active.cron = Set(normalize_optional_text(dto.cron));
        active.is_enable = Set(dto.is_enable);
        active.remark = Set(normalize_optional_text(dto.remark));
        apply_update_user_audit_field(&mut active.update_user_id, current_user);

        let updated_model = active
            .update(&tx)
            .await
            .map_err(|error| AppError::internal(format!("更新任务配置失败: {error}")))?;

        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "TaskSetting",
            task_setting_id,
            "update",
            Some(before_log),
            Some(json!({
                "id": updated_model.id,
                "name": updated_model.name,
                "category": updated_model.category,
                "semaphore": updated_model.semaphore,
                "cron": updated_model.cron,
                "parameter": updated_model.parameter,
                "is_enable": updated_model.is_enable,
                "remark": updated_model.remark,
            })),
            Some(current_user),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 获取支持参数的定时任务名称列表
    pub async fn get_args_schedule_task_name_list(&self) -> Result<Vec<String>, AppError> {
        task_setting::Entity::find()
            .not_deleted(task_setting::Column::DeleteTime)
            .filter(task_setting::Column::Category.eq("ScheduleTask"))
            .filter(task_setting::Column::Parameter.eq(ARGS_DEFAULT_PARAMETER))
            .order_by_asc(task_setting::Column::Name)
            .all(self.context.db)
            .await
            .map(|items| items.into_iter().map(|item| item.name).collect())
            .map_err(to_app_error)
    }

    /// 创建带参定时任务
    pub async fn create_schedule_task(
        &self,
        dto: CreateScheduleTaskDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        validate_create_schedule_task(&dto)?;

        let template = task_setting::Entity::find()
            .not_deleted(task_setting::Column::DeleteTime)
            .filter(task_setting::Column::Category.eq("ScheduleTask"))
            .filter(task_setting::Column::Name.eq(dto.name.trim()))
            .filter(task_setting::Column::Parameter.eq(ARGS_DEFAULT_PARAMETER))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(template) = template else {
            return Err(AppError::bad_request(
                "任务名称不存在，请先确认 TaskService 已同步任务配置",
            ));
        };

        let tx = begin_transaction(self.context.db).await?;
        let task_setting_id = self
            .context
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成任务配置主键失败: {error}")))?;
        let normalized_cron = normalize_optional_text(dto.cron);
        let normalized_remark = normalize_optional_text(dto.remark);
        let audit_after = json!({
            "id": task_setting_id,
            "name": dto.name,
            "category": "ScheduleTask",
            "cron": normalized_cron,
            "parameter": dto.parameter,
            "is_enable": dto.is_enable,
            "remark": normalized_remark,
        });
        let audit_fields = build_create_full_audit_fields(current_user);
        let task_model = task_setting::ActiveModel {
            id: Set(task_setting_id),
            name: Set(template.name),
            category: Set("ScheduleTask".to_string()),
            semaphore: Set(template.semaphore),
            cron: Set(normalized_cron.or(template.cron)),
            remark: Set(normalized_remark),
            create_time: Set(audit_fields.create_time),
            delete_time: Set(audit_fields.delete_time),
            update_time: Set(audit_fields.update_time),
            create_user_id: Set(audit_fields.create_user_id),
            update_user_id: Set(audit_fields.update_user_id),
            delete_user_id: Set(audit_fields.delete_user_id),
            is_enable: Set(dto.is_enable),
            parameter: Set(Some(dto.parameter.trim().to_string())),
        };

        task_model
            .insert(&tx)
            .await
            .map_err(|error| AppError::internal(format!("创建带参定时任务失败: {error}")))?;

        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "TaskSetting",
            task_setting_id,
            "create",
            None,
            Some(audit_after),
            Some(current_user),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(task_setting_id.to_string())
    }

    /// 删除任务配置信息
    pub async fn delete_task_setting(
        &self,
        task_setting_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        validate_task_setting_id(task_setting_id)?;

        let task_model = task_setting::Entity::find_by_id(task_setting_id)
            .not_deleted(task_setting::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(task_model) = task_model else {
            return Err(AppError::bad_request("无效的 taskSettingId"));
        };

        if !is_dynamic_schedule_task(&task_model) {
            return Err(AppError::bad_request("仅允许删除动态创建的带参定时任务"));
        }

        let before_log = json!({
            "id": task_model.id,
            "name": task_model.name,
            "category": task_model.category,
            "parameter": task_model.parameter,
            "is_enable": task_model.is_enable,
            "remark": task_model.remark,
        });
        let tx = begin_transaction(self.context.db).await?;
        let mut active = task_model.into_active_model();
        active.is_enable = Set(false);
        apply_soft_delete_audit_fields(
            &mut active.delete_time,
            &mut active.delete_user_id,
            current_user,
        );
        apply_update_user_audit_field(&mut active.update_user_id, current_user);

        active
            .update(&tx)
            .await
            .map_err(|error| AppError::internal(format!("删除任务配置失败: {error}")))?;

        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "TaskSetting",
            task_setting_id,
            "delete",
            Some(before_log),
            Some(json!({
                "id": task_setting_id,
                "delete_user_id": current_user.user_id,
            })),
            Some(current_user),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 获取队列任务列表
    pub async fn get_queue_task_list(
        &self,
        params: QueueTaskPageRequest,
    ) -> Result<QueueTaskPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query = queue_task::Entity::find().not_deleted(queue_task::Column::DeleteTime);

        if let Some(is_success) = params.is_success {
            query = if is_success {
                query.filter(queue_task::Column::SuccessTime.is_not_null())
            } else {
                query.filter(queue_task::Column::SuccessTime.is_null())
            };
        }

        if let Some(start_create_time) = params.start_create_time {
            query = query.filter(queue_task::Column::CreateTime.gte(start_create_time));
        }

        if let Some(end_create_time) = params.end_create_time {
            query = query.filter(queue_task::Column::CreateTime.lte(end_create_time));
        }

        if let Some(parameter_keyword) = params
            .parameter_keyword
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            query = query.filter(queue_task::Column::Parameter.contains(parameter_keyword));
        }

        let total = query
            .clone()
            .count(self.context.db)
            .await
            .map_err(to_app_error)?;

        if total == 0 {
            return Ok(PageListDto {
                total: 0,
                list: vec![],
            });
        }

        let page = normalize_page(params.page);
        let skip = (page - 1) * params.page_size;
        let list = query
            .order_by_desc(queue_task::Column::Id)
            .offset(skip)
            .limit(params.page_size)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(build_queue_task_dto)
            .collect();

        Ok(PageListDto { total, list })
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
            .count(self.context.db)
            .await
            .map_err(to_app_error)?;

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
            .map_err(to_app_error)?
            .into_iter()
            .map(build_task_execution_log_dto)
            .collect();

        Ok(PageListDto { total, list })
    }

    /// 在外部事务中创建队列任务
    ///
    /// 与 NetEngine 的 Create 语义对齐：调用方负责开启并提交事务。
    pub async fn create_queue_task(
        &self,
        tx: &DatabaseTransaction,
        dto: CreateQueueTaskDto,
    ) -> Result<String, AppError> {
        validate_create_queue_task(&dto)?;

        self.create_queue_task_with_connection(tx, dto).await
    }

    /// 单独创建队列任务
    pub async fn create_queue_task_single(
        &self,
        dto: CreateQueueTaskDto,
    ) -> Result<String, AppError> {
        validate_create_queue_task(&dto)?;

        let tx = begin_transaction(self.context.db).await?;
        let task_id = self.create_queue_task_with_connection(&tx, dto).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(task_id)
    }

    /// 使用指定连接写入队列任务
    async fn create_queue_task_with_connection<C>(
        &self,
        db: &C,
        dto: CreateQueueTaskDto,
    ) -> Result<String, AppError>
    where
        C: ConnectionTrait,
    {
        let task_id = self
            .context
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成任务主键失败: {error}")))?;
        let task_model = queue_task::ActiveModel {
            id: Set(task_id),
            name: Set(dto.name.trim().to_string()),
            parameter: Set(normalize_optional_text(dto.parameter)),
            status: Set(QueueTaskStatus::Pending),
            count: Set(0),
            first_time: Set(None),
            last_time: Set(None),
            success_time: Set(None),
            create_time: Set(Utc::now().into()),
            delete_time: Set(None),
            plan_time: Set(dto.plan_time.map(Into::into)),
            worker_id: Set(None),
            lease_expire_time: Set(None),
            callback_name: Set(normalize_optional_text(dto.callback_name)),
            callback_parameter: Set(normalize_optional_text(dto.callback_parameter)),
            child_success_time: Set(None),
            parent_task_id: Set(dto.parent_task_id),
        };

        task_model
            .insert(db)
            .await
            .map_err(|error| AppError::internal(format!("创建队列任务失败: {error}")))?;

        Ok(task_id.to_string())
    }

    /// 重试队列任务
    pub async fn retry_queue_task(&self, id: i64) -> Result<bool, AppError> {
        validate_queue_task_id(id)?;

        let task_model = queue_task::Entity::find_by_id(id)
            .not_deleted(queue_task::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(task_model) = task_model else {
            return Err(AppError::bad_request("无效的 QueueTaskId"));
        };

        if task_model.success_time.is_some() {
            return Err(AppError::bad_request("已成功的任务不允许重试"));
        }

        let tx = begin_transaction(self.context.db).await?;
        let mut active = task_model.into_active_model();
        active.status = Set(QueueTaskStatus::Pending);
        active.plan_time = Set(None);
        active.count = Set(0);
        active.first_time = Set(None);
        active.last_time = Set(None);
        active.success_time = Set(None);
        active.child_success_time = Set(None);
        active.worker_id = Set(None);
        active.lease_expire_time = Set(None);

        active
            .update(&tx)
            .await
            .map_err(|error| AppError::internal(format!("重试队列任务失败: {error}")))?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }
}

/// 校验任务配置标识
fn validate_task_setting_id(task_setting_id: i64) -> Result<(), AppError> {
    if task_setting_id <= 0 {
        return Err(AppError::bad_request("无效的 taskSettingId"));
    }

    Ok(())
}

/// 校验队列任务标识
fn validate_queue_task_id(id: i64) -> Result<(), AppError> {
    if id <= 0 {
        return Err(AppError::bad_request("无效的 QueueTaskId"));
    }

    Ok(())
}

/// 校验更新任务配置请求
fn validate_edit_task_setting(
    dto: &EditTaskSettingDto,
    is_args_template: bool,
) -> Result<(), AppError> {
    if let Some(semaphore) = dto.semaphore {
        if semaphore <= 0 {
            return Err(AppError::bad_request("semaphore 必须大于 0"));
        }
    }

    if let Some(parameter) = dto
        .parameter
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if is_args_template && parameter == ARGS_DEFAULT_PARAMETER {
            return Ok(());
        }

        parse_json_text(parameter, "任务参数必须是合法 JSON")?;
    }

    Ok(())
}

/// 校验创建带参定时任务请求
fn validate_create_schedule_task(dto: &CreateScheduleTaskDto) -> Result<(), AppError> {
    if dto.name.trim().is_empty() {
        return Err(AppError::bad_request("name 不可以空"));
    }

    if dto.parameter.trim().is_empty() {
        return Err(AppError::bad_request("parameter 不可以空"));
    }

    parse_json_text(dto.parameter.trim(), "parameter 必须是合法 JSON")?;

    Ok(())
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

    if let Some(plan_time) = dto.plan_time {
        if plan_time <= Utc::now().fixed_offset() {
            return Err(AppError::bad_request("planTime 必须大于当前时间"));
        }
    }

    if let Some(parent_task_id) = dto.parent_task_id {
        if parent_task_id <= 0 {
            return Err(AppError::bad_request("parentTaskId 必须大于 0"));
        }
    }

    Ok(())
}

/// 解析 JSON 文本
fn parse_json_text(value: &str, error_message: &str) -> Result<(), AppError> {
    serde_json::from_str::<serde_json::Value>(value)
        .map(|_| ())
        .map_err(|_| AppError::bad_request(error_message))
}

/// 判断是否为动态带参定时任务
fn is_dynamic_schedule_task(model: &task_setting::Model) -> bool {
    model.category == "ScheduleTask"
        && model
            .parameter
            .as_deref()
            .is_some_and(|item| !item.is_empty() && item != ARGS_DEFAULT_PARAMETER)
}

/// 判断是否为带参定时任务模板
fn is_args_schedule_task_template(model: &task_setting::Model) -> bool {
    model.category == "ScheduleTask" && model.parameter.as_deref() == Some(ARGS_DEFAULT_PARAMETER)
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

/// 转换数据库异常
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}
