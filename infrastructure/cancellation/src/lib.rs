//! 跨组件的协作取消契约，不依赖应用模型或方法代理。

pub use tokio_util::sync::CancellationToken;

/// 当前操作已取消；转换为组件错误时须保留取消分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationCancelled;

impl std::fmt::Display for OperationCancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("操作已取消")
    }
}

impl std::error::Error for OperationCancelled {}

/// 识别当前错误是否代表操作取消，不按消息文本或内部错误链推断。
pub trait CancellationError {
    fn is_cancelled(&self) -> bool;
}

impl CancellationError for OperationCancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

/// 在操作边界观察取消；不会主动中断已开始的业务 future。
pub fn check(token: &CancellationToken) -> Result<(), OperationCancelled> {
    if token.is_cancelled() {
        Err(OperationCancelled)
    } else {
        Ok(())
    }
}
