use std::{error::Error, fmt};

/// 通用自定义异常
#[derive(Debug)]
pub struct CustomError {
    /// 错误消息
    message: String,

    /// 内部错误
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl CustomError {
    /// 创建自定义异常
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// 创建带内部错误的自定义异常
    pub fn with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// 获取错误消息
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for CustomError {
    /// 输出异常消息
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for CustomError {
    /// 返回内部异常
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|error| error as &(dyn Error + 'static))
    }
}
