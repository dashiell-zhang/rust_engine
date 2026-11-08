use thiserror::Error;

/// LLM 操作错误类型
#[derive(Debug, Error)]
pub enum LlmError {
    /// 调用方主动取消。
    #[error("LLM 操作已取消")]
    Cancelled,

    /// HTTP 请求失败
    #[error("HTTP 请求失败: {0}")]
    Http(String),

    /// JSON 序列化或反序列化失败
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// 提供商返回的响应内容或字符集无效。
    #[error("{0}")]
    InvalidResponse(String),

    /// API 返回非成功状态码
    #[error("{message}")]
    ApiError {
        /// HTTP 状态码
        status: u16,
        /// HTTP 状态失败描述；不读取错误正文。
        message: String,
    },

    /// 流式响应行解析失败
    #[error("流式响应解析失败: {0}")]
    StreamParse(String),

    /// 不支持的协议类型
    #[error("不支持的 LLM 协议类型: {0}")]
    UnsupportedProtocol(i32),

    /// 请求参数无效
    #[error("LLM 请求参数无效: {0}")]
    InvalidRequest(String),
}

impl From<reqwest::Error> for LlmError {
    /// 将 reqwest HTTP 错误转换为 LlmError
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error.to_string())
    }
}

impl cancellation::CancellationError for LlmError {
    fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

impl From<cancellation::OperationCancelled> for LlmError {
    fn from(_: cancellation::OperationCancelled) -> Self {
        Self::Cancelled
    }
}
