use std::fmt::{Display, Formatter};

/// 应用异常分类
#[derive(Debug)]
pub enum AppErrorKind {
    /// 参数或业务校验错误
    BadRequest,

    /// 未授权错误
    Unauthorized,

    /// 资源冲突错误
    Conflict,

    /// 服务器内部错误
    Internal,

    /// 调用方取消，不能作为普通失败吞掉。
    Cancelled,
}

/// 统一业务异常
#[derive(Debug)]
pub struct AppError {
    /// 错误分类
    kind: AppErrorKind,

    /// 错误消息
    message: String,
}

impl AppError {
    /// 创建业务异常
    pub fn new(msg: impl Into<String>) -> Self {
        Self::bad_request(msg)
    }

    /// 创建参数错误
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::BadRequest,
            message: msg.into(),
        }
    }

    /// 创建未授权错误
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::Unauthorized,
            message: msg.into(),
        }
    }

    /// 创建冲突错误
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::Conflict,
            message: msg.into(),
        }
    }

    /// 创建服务器内部错误
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            kind: AppErrorKind::Internal,
            message: msg.into(),
        }
    }

    /// 创建调用取消错误。
    pub fn cancelled() -> Self {
        Self {
            kind: AppErrorKind::Cancelled,
            message: "操作已取消".into(),
        }
    }

    /// 获取错误分类
    pub fn kind(&self) -> &AppErrorKind {
        &self.kind
    }

    /// 获取错误消息
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for AppError {
    /// 输出业务异常消息
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl cancellation::CancellationError for AppError {
    fn is_cancelled(&self) -> bool {
        matches!(self.kind, AppErrorKind::Cancelled)
    }
}

impl From<cancellation::OperationCancelled> for AppError {
    fn from(_: cancellation::OperationCancelled) -> Self {
        Self::cancelled()
    }
}

#[cfg(feature = "runtime")]
impl From<distributed_cache::CacheError> for AppError {
    /// 将缓存异常转换为应用层内部错误
    fn from(value: distributed_cache::CacheError) -> Self {
        Self::internal(format!("缓存操作失败: {value}"))
    }
}

#[cfg(feature = "runtime")]
impl From<distributed_lock::LockError> for AppError {
    /// 将分布式锁异常转换为应用层内部错误
    fn from(value: distributed_lock::LockError) -> Self {
        Self::internal(format!("分布式锁操作失败: {value}"))
    }
}

impl From<String> for AppError {
    /// 将字符串转换为参数错误
    fn from(value: String) -> Self {
        Self::bad_request(value)
    }
}

impl From<&str> for AppError {
    /// 将字符串切片转换为参数错误
    fn from(value: &str) -> Self {
        Self::bad_request(value)
    }
}
