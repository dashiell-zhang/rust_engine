use thiserror::Error;

/// LLM 操作错误类型
#[derive(Debug, Error)]
pub enum LlmError {
    /// HTTP 请求失败
    #[error("HTTP 请求失败: {0}")]
    Http(String),

    /// JSON 序列化或反序列化失败
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// API 返回非成功状态码
    #[error("API 返回错误: {status} - {message}")]
    ApiError {
        /// HTTP 状态码
        status: u16,
        /// 响应体或描述
        message: String,
    },

    /// 流式响应行解析失败
    #[error("流式响应解析失败: {0}")]
    StreamParse(String),

    /// 指定 Provider 未注册
    #[error("Provider '{0}' 未注册")]
    ProviderNotFound(String),
}

impl From<reqwest::Error> for LlmError {
    /// 将 reqwest HTTP 错误转换为 LlmError
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error.to_string())
    }
}
