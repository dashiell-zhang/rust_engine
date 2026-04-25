use thiserror::Error;

/// 分布式锁操作的错误类型
#[derive(Debug, Error)]
pub enum LockError {
    /// 在 expiry 超时时间内未能获取到指定 key 的锁
    ///
    /// `key` 字段记录超时的锁名称，便于排查哪把锁发生了竞争
    #[error("获取锁 {key} 超时失败")]
    Timeout {
        /// 超时的锁名称
        key: String,
    },

    /// 调用方传入了无效参数，如空 key 或 semaphore 为 0
    ///
    /// `message` 字段描述具体的参数错误原因
    #[error("锁参数无效: {message}")]
    InvalidArgument {
        /// 参数错误的具体描述
        message: String,
    },

    /// 锁操作内部错误，由底层提供者（如 Redis）返回的错误转换而来
    #[error("锁内部错误: {0}")]
    InternalError(String),
}
