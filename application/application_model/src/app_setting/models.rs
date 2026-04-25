use common::json_helper::deserialize_option_i64_from_string_or_number;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 应用配置项返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingDto {
    /// 配置 ID
    pub id: String,

    /// 配置模块
    pub module: String,

    /// 配置分组 ID
    pub group_id: Option<i64>,

    /// 配置键
    pub key: String,

    /// 配置值
    pub value: String,

    /// 创建时间
    pub create_time: String,
}

/// 写入配置项请求模型
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetAppSettingDto {
    /// 配置模块
    pub module: String,

    /// 配置键
    pub key: String,

    /// 配置值
    pub value: String,

    /// 配置分组 ID
    #[serde(
        default,
        deserialize_with = "deserialize_option_i64_from_string_or_number"
    )]
    pub group_id: Option<i64>,
}

/// 获取配置列表的查询参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetAppSettingListQuery {
    /// 配置模块
    pub module: String,
}

/// 获取单个配置的查询参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetAppSettingQuery {
    /// 配置模块
    pub module: String,

    /// 配置键
    pub key: String,
}

/// 获取字典值列表查询参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetValueListQuery {
    /// 字典分组 ID
    #[serde(rename = "groupId")]
    pub group_id: i64,
}

/// 删除配置项的查询参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAppSettingQuery {
    /// 配置模块
    pub module: String,

    /// 配置键
    pub key: String,
}
