use crate::shared::models::{PageListDto, default_page, default_page_size};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

/// LLM 模型配置分页请求
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelPageRequest {
    /// 页码
    #[param(minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u64,

    /// 每页条数
    #[param(minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    pub page_size: u64,

    /// 关键词，匹配名称、模型标识或备注
    pub keyword: Option<String>,

    /// 是否启用
    #[serde(rename = "isEnable")]
    pub is_enable: Option<bool>,
}

impl crate::shared::models::PageRequest for LlmModelPageRequest {
    fn page(&self) -> u64 {
        self.page
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}

/// LLM 模型配置 ID 参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct LlmModelIdQuery {
    /// LLM 模型配置 ID
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
    pub create_time: String,

    /// 更新时间
    pub update_time: Option<String>,
}

/// 创建或更新 LLM 模型配置请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditLlmModelDto {
    /// 模型显示名称
    pub name: String,

    /// 模型标识
    pub model_id: String,

    /// API 端点地址
    pub endpoint: String,

    /// 接口密钥，更新时为空则不修改
    pub api_key: Option<String>,

    /// 协议类型（0=Chat, 1=Responses, 2=Anthropic）
    pub protocol_type: i32,

    /// 是否启用
    pub is_enable: bool,

    /// 备注
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
#[serde(rename_all = "camelCase")]
pub struct LlmAppPageRequest {
    /// 页码
    #[param(minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u64,

    /// 每页条数
    #[param(minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    pub page_size: u64,

    /// 关键词，匹配 Code、名称或备注
    pub keyword: Option<String>,

    /// 是否启用
    #[serde(rename = "isEnable")]
    pub is_enable: Option<bool>,
}

impl crate::shared::models::PageRequest for LlmAppPageRequest {
    fn page(&self) -> u64 {
        self.page
    }

    fn page_size(&self) -> u64 {
        self.page_size
    }
}

/// LLM 应用配置 ID 参数
#[derive(Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct LlmAppIdQuery {
    /// LLM 应用配置 ID
    #[serde(rename = "llmAppId")]
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
    pub create_time: String,

    /// 更新时间
    pub update_time: Option<String>,
}

/// 创建或更新 LLM 应用配置请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditLlmAppDto {
    /// 应用标记
    pub code: String,

    /// 应用名称
    pub name: String,

    /// 关联的 LLM 模型 ID
    pub llm_model_id: i64,

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
}

/// LLM 应用配置分页返回模型
pub type LlmAppPageListDto = PageListDto<LlmAppDto>;

/// LLM 应用调用测试请求
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestLlmAppRequest {
    /// 关联的 LLM 模型 ID
    pub llm_model_id: i64,

    /// System 提示词模板
    pub system_prompt_template: Option<String>,

    /// User 提示词模板
    pub prompt_template: String,

    /// 额外请求参数
    pub extra_body: Option<Value>,

    /// 模板参数
    pub parameters: Option<std::collections::HashMap<String, String>>,
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
#[serde(rename_all = "camelCase")]
pub struct LlmConversationPageRequest {
    /// 页码，从 1 开始
    #[param(minimum = 1)]
    #[serde(default = "default_page")]
    pub page: u64,

    /// 每页条数
    #[param(minimum = 1, maximum = 100)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    pub page_size: u64,

    /// 关键词，匹配应用 Code、应用名称、系统提示词、用户消息或助手回复
    pub keyword: Option<String>,

    /// 开始时间
    #[serde(rename = "startTime")]
    pub start_time: Option<chrono::DateTime<chrono::FixedOffset>>,

    /// 结束时间
    #[serde(rename = "endTime")]
    pub end_time: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl crate::shared::models::PageRequest for LlmConversationPageRequest {
    fn page(&self) -> u64 {
        self.page
    }

    fn page_size(&self) -> u64 {
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

    /// 系统提示词内容
    pub system_content: String,

    /// 用户消息内容
    pub user_content: String,

    /// 助手回复内容
    pub assistant_content: String,

    /// 创建人用户 ID
    pub create_user_id: Option<String>,

    /// 创建人名称
    pub create_user_name: Option<String>,

    /// 创建时间
    pub create_time: String,
}

/// LLM 对话记录分页返回模型
pub type LlmConversationPageListDto = PageListDto<LlmConversationDto>;
