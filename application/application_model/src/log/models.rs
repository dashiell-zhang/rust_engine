use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::shared::models::{PageListDto, default_page, default_page_size};

/// 日志分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct LogPageRequest {
    /// 页码，默认 1
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 项目名称关键词
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub project: Option<String>,

    /// 机器名称关键词
    #[serde(rename = "machineName")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub machine_name: Option<String>,

    /// 日志级别
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub level: Option<String>,

    /// 内容关键词
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub content: Option<String>,

    /// 创建开始时间
    #[serde(rename = "startTime")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::input::deserialize_optional_query_date_time"
    )]
    pub start_time: Option<DateTime<FixedOffset>>,

    /// 创建结束时间
    #[serde(rename = "endTime")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::input::deserialize_optional_query_date_time"
    )]
    pub end_time: Option<DateTime<FixedOffset>>,
}

impl crate::shared::models::PageRequest for LogPageRequest {
    const QUERY_PROPERTIES: &'static [(&'static str, &'static str)] =
        &[("startTime", "StartTime"), ("endTime", "EndTime")];
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
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
    #[serde(serialize_with = "common::json_helper::date_time::serialize_offset_string")]
    pub create_time: String,
}

/// 日志分页返回模型
pub type LogPageListDto = PageListDto<LogDto>;
