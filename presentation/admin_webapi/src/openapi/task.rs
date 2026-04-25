use application_model::task_center::models::{
    CreateScheduleTaskDto, EditTaskSettingDto, QueueTaskDto, QueueTaskPageListDto, TaskSettingDto,
    TaskSettingPageListDto,
};
use utoipa::OpenApi;

use crate::handlers::task;

/// 任务中心模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        task::get_task_setting_list,
        task::update_task_setting,
        task::get_args_schedule_task_name_list,
        task::create_schedule_task,
        task::get_queue_task_list,
        task::retry_queue_task,
    ),
    components(
        schemas(
            TaskSettingPageListDto,
            TaskSettingDto,
            EditTaskSettingDto,
            CreateScheduleTaskDto,
            QueueTaskPageListDto,
            QueueTaskDto,
        )
    ),
    tags(
        (name = "任务中心", description = "任务配置与队列任务接口"),
    )
)]
pub struct TaskApiDoc;
