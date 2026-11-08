use crate::shared::models::{PageListDto, default_page, default_page_size};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// LLM 模型配置分页请求
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct LlmModelPageRequest {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 关键词，匹配名称、模型标识或备注
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub keyword: Option<String>,

    /// 是否启用
    #[serde(rename = "isEnable")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub is_enable: Option<bool>,
}

impl crate::shared::models::PageRequest for LlmModelPageRequest {
    const QUERY_PROPERTIES: &'static [(&'static str, &'static str)] = &[("isEnable", "IsEnable")];

    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// LLM 模型配置 ID 参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct LlmModelIdQuery {
    /// LLM 模型配置 ID
    #[serde(default)]
    pub id: i64,
}

/// LLM 模型配置返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelDto {
    /// ID
    pub id: String,

    /// 模型显示名称
    pub name: String,

    /// 模型标识
    pub model_id: String,

    /// API 端点地址
    pub endpoint: String,

    /// 协议类型（0=Chat, 1=Responses, 2=Anthropic）
    pub protocol_type: i32,

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

/// 创建或更新 LLM 模型配置请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.LLM.LlmModel.EditLlmModelDto")]
pub struct EditLlmModelDto {
    /// 模型显示名称
    #[json(required = "名称不可以空")]
    pub name: String,

    /// 模型标识
    #[json(required = "模型标识不可以空")]
    pub model_id: String,

    /// API 端点地址
    #[json(required = "端点地址不可以空")]
    pub endpoint: String,

    /// 接口密钥，更新时为空则不修改
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub api_key: Option<String>,

    /// 协议类型（0=Chat, 1=Responses, 2=Anthropic）
    #[serde(default)]
    #[schema(default = 0)]
    pub protocol_type: i32,

    /// 是否启用
    #[serde(default = "default_enabled")]
    #[schema(default = default_enabled)]
    pub is_enable: bool,

    /// 备注
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub remark: Option<String>,
}

/// LLM 模型下拉选项
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelSelectDto {
    /// 模型 ID
    pub id: String,

    /// 模型显示名称
    pub name: String,
}

/// LLM 模型配置分页返回模型
pub type LlmModelPageListDto = PageListDto<LlmModelDto>;

/// LLM 应用配置分页请求
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct LlmAppPageRequest {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 关键词，匹配 Code、名称或备注
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub keyword: Option<String>,

    /// 是否启用
    #[serde(rename = "isEnable")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub is_enable: Option<bool>,
}

impl crate::shared::models::PageRequest for LlmAppPageRequest {
    const QUERY_PROPERTIES: &'static [(&'static str, &'static str)] = &[("isEnable", "IsEnable")];

    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// LLM 应用配置 ID 参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct LlmAppIdQuery {
    /// LLM 应用配置 ID
    #[serde(rename = "id", default)]
    pub llm_app_id: i64,
}

/// LLM 应用配置返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LlmAppDto {
    /// ID
    pub id: String,

    /// 应用标记
    pub code: String,

    /// 应用名称
    pub name: String,

    /// 关联的 LLM 模型 ID
    pub llm_model_id: String,

    /// 关联的 LLM 模型名称
    pub llm_model_name: String,

    /// System 提示词模板
    pub system_prompt_template: Option<String>,

    /// User 提示词模板
    pub prompt_template: String,

    /// 额外请求参数 JSON 字符串
    pub extra_body_json: Option<String>,

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

/// 创建或更新 LLM 应用配置请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.LLM.LlmApp.EditLlmAppDto")]
pub struct EditLlmAppDto {
    /// 应用标记
    #[json(required = "Code 不可以空")]
    pub code: String,

    /// 应用名称
    #[json(required = "名称不可以空")]
    pub name: String,

    /// 关联的 LLM 模型 ID
    #[serde(default)]
    pub llm_model_id: i64,

    /// System 提示词模板
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub system_prompt_template: Option<String>,

    /// User 提示词模板
    #[json(required = "PromptTemplate 不可以空")]
    pub prompt_template: String,

    /// 额外请求参数 JSON 字符串
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub extra_body_json: Option<String>,

    /// 是否启用
    #[serde(default = "default_enabled")]
    #[schema(default = default_enabled)]
    pub is_enable: bool,

    /// 备注
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub remark: Option<String>,
}

/// LLM 应用配置分页返回模型
pub type LlmAppPageListDto = PageListDto<LlmAppDto>;

/// LLM 应用调用测试请求
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.LLM.LlmApp.TestLlmAppRequestDto")]
pub struct TestLlmAppRequest {
    /// 关联的 LLM 模型 ID
    #[serde(default)]
    pub llm_model_id: i64,

    /// System 提示词模板
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_json_scalar"
    )]
    pub system_prompt_template: Option<String>,

    /// User 提示词模板
    pub prompt_template: String,

    /// 额外请求参数
    #[json(dictionary_value_name = "System.Text.Json.Nodes.JsonNode")]
    #[serde(default, deserialize_with = "deserialize_llm_extra_body")]
    #[schema(value_type = Option<std::collections::HashMap<String, serde_json::Value>>)]
    pub extra_body: Option<indexmap::IndexMap<String, Box<serde_json::value::RawValue>>>,

    /// 模板参数
    #[serde(default, deserialize_with = "deserialize_llm_parameters")]
    pub parameters: Option<std::collections::HashMap<String, Option<String>>>,
}

/// LLM 应用调用测试响应
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestLlmAppResult {
    /// 实际使用模型
    pub model: String,

    /// 响应 ID
    pub response_id: Option<String>,

    /// 返回内容
    pub content: Option<String>,

    /// Token 用量
    pub usage: Option<TestLlmAppUsage>,
}

/// LLM 应用调用测试用量
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestLlmAppUsage {
    /// 提示词 token 数
    pub prompt_tokens: Option<i32>,

    /// 输出 token 数
    pub completion_tokens: Option<i32>,

    /// 总 token 数
    pub total_tokens: Option<i32>,
}

/// LLM 对话记录分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct LlmConversationPageRequest {
    /// 页码，默认 1
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 关键词，匹配应用 Code、应用名称、系统提示词、用户消息或助手回复
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub keyword: Option<String>,

    /// 开始时间
    #[serde(rename = "startTime")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::input::deserialize_optional_query_date_time"
    )]
    pub start_time: Option<chrono::DateTime<chrono::FixedOffset>>,

    /// 结束时间
    #[serde(rename = "endTime")]
    #[serde(
        default,
        deserialize_with = "common::json_helper::input::deserialize_optional_query_date_time"
    )]
    pub end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl crate::shared::models::PageRequest for LlmConversationPageRequest {
    const QUERY_PROPERTIES: &'static [(&'static str, &'static str)] =
        &[("startTime", "StartTime"), ("endTime", "EndTime")];
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// LLM 对话记录返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LlmConversationDto {
    /// 记录 ID
    pub id: String,

    /// 关联的 LLM 应用配置 ID
    pub llm_app_id: String,

    /// 关联的 LLM 应用标记
    pub llm_app_code: String,

    /// 关联的 LLM 应用名称
    pub llm_app_name: String,

    /// 创建人用户 ID
    pub create_user_id: Option<String>,

    /// 创建人名称
    pub create_user_name: Option<String>,

    /// 创建时间
    #[serde(serialize_with = "common::json_helper::date_time::serialize_offset_string")]
    pub create_time: String,

    /// 系统提示词内容
    pub system_content: String,

    /// 用户消息内容
    pub user_content: String,

    /// 助手回复内容
    pub assistant_content: String,
}

/// LLM 对话记录分页返回模型
pub type LlmConversationPageListDto = PageListDto<LlmConversationDto>;

/// 与源编辑 DTO 的 IsEnable 初始化值一致。
fn default_enabled() -> bool {
    true
}

/// 模板参数是用户字典，保留空字符串、null 和键名，不套用 DTO 必填字符串校验。
fn deserialize_llm_parameters<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<std::collections::HashMap<String, Option<String>>>, D::Error> {
    let value = common::json_helper::input::deserialize_json_value(deserializer)?;
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

/// 扩展请求体必须为对象；对象的动态键和值直接保留给协议层。
fn deserialize_llm_extra_body<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<super::invocation::LlmExtraBody>, D::Error> {
    Option::<super::invocation::LlmExtraBody>::deserialize(deserializer)
}
