use chrono::{DateTime, FixedOffset};
use common::json_helper::deserialize_option_i64_from_string_or_number;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

use crate::shared::models::{PageListDto, default_page, default_page_size};

/// 任务配置分页请求参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct TaskSettingPageRequest {
    /// 页码
    #[param(minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u64,

    /// 每页条数
    #[param(minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    pub page_size: u64,

    /// 任务分类
    pub category: Option<String>,
}

impl crate::shared::models::PageRequest for TaskSettingPageRequest {
    fn page(&self) -> u64 {
        self.page
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}

/// 队列任务分页请求参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct QueueTaskPageRequest {
    /// 页码
    #[param(minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u64,

    /// 每页条数
    #[param(minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    pub page_size: u64,

    /// 是否成功
    #[serde(rename = "isSuccess")]
    pub is_success: Option<bool>,

    /// 参数关键词
    #[serde(rename = "parameterKeyword")]
    pub parameter_keyword: Option<String>,

    /// 创建开始时间
    #[serde(rename = "startCreateTime")]
    pub start_create_time: Option<DateTime<FixedOffset>>,

    /// 创建结束时间
    #[serde(rename = "endCreateTime")]
    pub end_create_time: Option<DateTime<FixedOffset>>,
}

impl crate::shared::models::PageRequest for QueueTaskPageRequest {
    fn page(&self) -> u64 {
        self.page
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}

/// 任务执行日志分页请求参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionLogPageRequest {
    /// 页码
    #[param(minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u64,

    /// 每页条数
    #[param(minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    pub page_size: u64,

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
    pub start_create_time: Option<DateTime<FixedOffset>>,

    /// 创建结束时间
    #[serde(rename = "endCreateTime")]
    pub end_create_time: Option<DateTime<FixedOffset>>,
}

impl crate::shared::models::PageRequest for TaskExecutionLogPageRequest {
    fn page(&self) -> u64 {
        self.page
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}

/// 任务配置标识参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct TaskSettingIdQuery {
    /// 任务配置 ID
    #[serde(rename = "taskSettingId")]
    pub task_setting_id: i64,
}

/// 队列任务标识参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct QueueTaskIdQuery {
    /// 队列任务 ID
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

    /// 并发信号量
    pub semaphore: Option<i32>,

    /// Cron 表达式
    pub cron: Option<String>,

    /// 任务参数
    pub parameter: Option<String>,

    /// 是否启用
    pub is_enable: bool,

    /// 备注
    pub remark: Option<String>,

    /// 创建时间
    pub create_time: String,

    /// 更新时间
    pub update_time: Option<String>,
}

/// 更新任务配置请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditTaskSettingDto {
    /// 并发信号量
    pub semaphore: Option<i32>,

    /// Cron 表达式
    pub cron: Option<String>,

    /// 任务参数
    pub parameter: Option<String>,

    /// 是否启用
    pub is_enable: bool,

    /// 备注
    pub remark: Option<String>,
}

/// 创建带参定时任务请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateScheduleTaskDto {
    /// 任务名称
    pub name: String,

    /// Cron 表达式
    pub cron: Option<String>,

    /// 任务参数 JSON
    pub parameter: String,

    /// 是否启用
    pub is_enable: bool,

    /// 备注
    pub remark: Option<String>,
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

    /// 任务参数
    pub parameter: Option<String>,

    /// 执行状态
    pub status: i32,

    /// 重试次数
    pub count: i32,

    /// 首次执行时间
    pub first_time: Option<String>,

    /// 最近执行时间
    pub last_time: Option<String>,

    /// 成功时间
    pub success_time: Option<String>,

    /// 计划执行时间
    pub plan_time: Option<String>,

    /// 当前执行实例标识
    pub worker_id: Option<String>,

    /// 执行租约过期时间
    pub lease_expire_time: Option<String>,

    /// 回调任务名称
    pub callback_name: Option<String>,

    /// 回调任务参数
    pub callback_parameter: Option<String>,

    /// 父任务 ID
    pub parent_task_id: Option<String>,

    /// 子任务成功时间
    pub child_success_time: Option<String>,

    /// 创建时间
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
