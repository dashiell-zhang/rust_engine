use thiserror::Error;

/// 分布式缓存操作的错误类型
#[derive(Debug, Error)]
pub enum CacheError {
    /// 调用方传入了无效参数
    #[error("缓存参数无效: {message}")]
    InvalidArgument {
        /// 参数错误的具体描述
        message: String,
    },

    /// JSON 序列化或反序列化失败
    #[error("缓存 JSON 序列化失败: {0}")]
    Json(#[from] serde_json::Error),

    /// 字节数组转 UTF 8 字符串失败
    #[error("缓存字符串解码失败: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    /// 缓存操作内部错误，由底层提供者（如 Redis）返回的错误转换而来
    #[error("缓存内部错误: {0}")]
    InternalError(String),
}
