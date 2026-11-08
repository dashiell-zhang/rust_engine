use chrono::{DateTime, FixedOffset};
use common::json_helper::deserialize_option_i64_from_string_or_number;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use crate::shared::models::{PageListDto, default_page, default_page_size};

/// 任务配置分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct TaskSettingPageRequest {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 任务分类
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub category: Option<String>,
}

impl crate::shared::models::PageRequest for TaskSettingPageRequest {
    const QUERY_PARAMETERS: &'static [&'static str] = &["category"];

    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 队列任务分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct QueueTaskPageRequest {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 是否成功
    #[serde(rename = "isSuccess")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub is_success: Option<bool>,

    /// 参数关键词
    #[serde(rename = "parameterKeyword")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub parameter_keyword: Option<String>,

    /// 创建开始时间
    #[serde(rename = "startCreateTime")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::input::deserialize_optional_query_date_time"
    )]
    pub start_create_time: Option<DateTime<FixedOffset>>,

    /// 创建结束时间
    #[serde(rename = "endCreateTime")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::input::deserialize_optional_query_date_time"
    )]
    pub end_create_time: Option<DateTime<FixedOffset>>,
}

impl crate::shared::models::PageRequest for QueueTaskPageRequest {
    const QUERY_PROPERTIES: &'static [(&'static str, &'static str)] = &[
        ("isSuccess", "IsSuccess"),
        ("startCreateTime", "StartCreateTime"),
        ("endCreateTime", "EndCreateTime"),
    ];

    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 任务执行日志分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct TaskExecutionLogPageRequest {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 任务 ID
    pub task_id: Option<i64>,

    /// 任务名称
    pub task_name: Option<String>,

    /// 执行结果
    pub result: Option<String>,

    /// 日志级别
    pub level: Option<String>,

    /// 任务类型 queue 或 schedule
    pub task_type: Option<String>,

    /// 创建开始时间
    #[serde(rename = "startCreateTime")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::input::deserialize_optional_query_date_time"
    )]
    pub start_create_time: Option<DateTime<FixedOffset>>,

    /// 创建结束时间
    #[serde(rename = "endCreateTime")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::input::deserialize_optional_query_date_time"
    )]
    pub end_create_time: Option<DateTime<FixedOffset>>,
}

impl crate::shared::models::PageRequest for TaskExecutionLogPageRequest {
    const QUERY_PROPERTIES: &'static [(&'static str, &'static str)] = &[
        ("startCreateTime", "StartCreateTime"),
        ("endCreateTime", "EndCreateTime"),
    ];
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 任务配置标识参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct TaskSettingIdQuery {
    /// 任务配置 ID
    #[serde(rename = "taskSettingId", default)]
    pub task_setting_id: i64,
}

/// 队列任务标识参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct QueueTaskIdQuery {
    /// 队列任务 ID
    #[serde(default)]
    pub id: i64,
}

/// 任务配置返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskSettingDto {
    /// 任务配置 ID
    pub id: String,

    /// 任务名称
    pub name: String,

    /// 任务分类
    pub category: String,

    /// 任务参数
    pub parameter: Option<String>,

    /// 并发信号量
    pub semaphore: Option<i32>,

    /// Cron 表达式
    pub cron: Option<String>,

    /// 是否启用
    pub is_enable: bool,

    /// 备注
    pub remark: Option<String>,

    /// 创建时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_offset_string")]
    pub create_time: String,

    /// 更新时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_optional_offset_string")]
    pub update_time: Option<String>,
}

/// 更新任务配置请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.TaskCenter.EditTaskSettingDto")]
pub struct EditTaskSettingDto {
    /// 并发信号量
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub semaphore: Option<i32>,

    /// Cron 表达式
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub cron: Option<String>,

    /// 任务参数
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub parameter: Option<String>,

    /// 是否启用
    #[serde(default)]
    pub is_enable: bool,

    /// 备注
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub remark: Option<String>,
}

/// 创建带参定时任务请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.TaskCenter.CreateScheduleTaskDto")]
pub struct CreateScheduleTaskDto {
    /// 任务名称
    #[json(required = "Name 不可以空")]
    pub name: String,

    /// Cron 表达式
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub cron: Option<String>,

    /// 任务参数 JSON
    #[json(required = "Parameter 不可以空")]
    pub parameter: String,

    /// 是否启用
    #[serde(default = "default_schedule_enabled")]
    pub is_enable: bool,

    /// 备注
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub remark: Option<String>,
}

fn default_schedule_enabled() -> bool {
    true
}

/// 创建队列任务请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateQueueTaskDto {
    /// 任务名称
    pub name: String,

    /// 任务参数 JSON
    pub parameter: Option<String>,

    /// 计划执行时间
    pub plan_time: Option<DateTime<FixedOffset>>,

    /// 回调任务名称
    pub callback_name: Option<String>,

    /// 回调任务参数 JSON
    pub callback_parameter: Option<String>,

    /// 父任务 ID
    #[serde(
        default,
        deserialize_with = "deserialize_option_i64_from_string_or_number"
    )]
    pub parent_task_id: Option<i64>,
}

/// 队列任务返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueTaskDto {
    /// 队列任务 ID
    pub id: String,

    /// 任务名称
    pub name: String,

    /// 执行状态
    pub status: i32,

    /// 当前执行实例标识
    pub worker_id: Option<String>,

    /// 任务参数
    pub parameter: Option<String>,

    /// 计划执行时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_optional_offset_string")]
    pub plan_time: Option<String>,

    /// 重试次数
    pub count: i32,

    /// 首次执行时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_optional_offset_string")]
    pub first_time: Option<String>,

    /// 最近执行时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_optional_offset_string")]
    pub last_time: Option<String>,

    /// 成功时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_optional_offset_string")]
    pub success_time: Option<String>,

    /// 执行租约过期时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_optional_offset_string")]
    pub lease_expire_time: Option<String>,

    /// 回调任务名称
    pub callback_name: Option<String>,

    /// 回调任务参数
    pub callback_parameter: Option<String>,

    /// 父任务 ID
    pub parent_task_id: Option<String>,

    /// 子任务成功时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_optional_offset_string")]
    pub child_success_time: Option<String>,

    /// 创建时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_offset_string")]
    pub create_time: String,
}

/// 任务执行日志返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionLogDto {
    /// 日志 ID
    pub id: String,

    /// 项目标识
    pub project: String,

    /// 机器名称
    pub machine_name: String,

    /// 日志级别
    pub level: String,

    /// 日志分类
    pub category: String,

    /// 任务 ID
    #[schema(value_type = Option<String>)]
    pub task_id: Option<i64>,

    /// 任务名称
    pub task_name: Option<String>,

    /// 执行结果
    pub result: Option<String>,

    /// 错误信息
    pub error: Option<String>,

    /// 日志内容原文
    pub content: String,

    /// 日志内容 JSON
    pub content_json: Option<Value>,

    /// 创建时间
    pub create_time: String,
}

/// 任务配置分页返回类型
pub type TaskSettingPageListDto = PageListDto<TaskSettingDto>;

/// 队列任务分页返回类型
pub type QueueTaskPageListDto = PageListDto<QueueTaskDto>;

/// 任务执行日志分页返回类型
pub type TaskExecutionLogPageListDto = PageListDto<TaskExecutionLogDto>;
