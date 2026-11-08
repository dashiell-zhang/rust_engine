use application_model::shared::models::PageListDto;
use application_model::task_center::models::{
    CreateScheduleTaskDto, EditTaskSettingDto, QueueTaskDto, TaskSettingDto,
};
use utoipa::OpenApi;

/// 任务中心模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    components(
        schemas(
            PageListDto<TaskSettingDto>,
            TaskSettingDto,
            EditTaskSettingDto,
            CreateScheduleTaskDto,
            PageListDto<QueueTaskDto>,
            QueueTaskDto,
        )
    ),
    tags(
        (name = "任务中心", description = "任务配置与队列任务接口"),
    )
)]
pub struct TaskApiDoc;
