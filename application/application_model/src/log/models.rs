use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::shared::models::{PageListDto, default_page, default_page_size};

/// 日志分页请求参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct LogPageRequest {
    /// 页码，从 1 开始
    #[param(minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u64,

    /// 每页条数
    #[param(minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    pub page_size: u64,

    /// 项目名称关键词
    pub project: Option<String>,

    /// 机器名称关键词
    #[serde(rename = "machineName")]
    pub machine_name: Option<String>,

    /// 日志级别
    pub level: Option<String>,

    /// 内容关键词
    pub content: Option<String>,

    /// 创建开始时间
    #[serde(rename = "startTime")]
    pub start_time: Option<DateTime<FixedOffset>>,

    /// 创建结束时间
    #[serde(rename = "endTime")]
    pub end_time: Option<DateTime<FixedOffset>>,
}

impl crate::shared::models::PageRequest for LogPageRequest {
    fn page(&self) -> u64 {
        self.page
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}

/// 日志返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LogDto {
    /// 日志 ID
    pub id: String,

    /// 项目名称
    pub project: String,

    /// 机器名称
    pub machine_name: String,

    /// 日志级别
    pub level: String,

    /// 日志分类
    pub category: String,

    /// 日志内容
    pub content: String,

    /// 创建时间
    pub create_time: String,
}

/// 日志分页返回模型
pub type LogPageListDto = PageListDto<LogDto>;
