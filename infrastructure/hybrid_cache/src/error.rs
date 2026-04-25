use distributed_cache::CacheError;

/// 混合缓存错误类型
#[derive(Debug, thiserror::Error)]
pub enum HybridCacheError {
    /// L2 分布式缓存操作失败
    #[error("L2 缓存操作失败: {0}")]
    L2(String),

    /// JSON 序列化或反序列化失败
    #[error("JSON 序列化/反序列化失败: {0}")]
    Json(#[from] serde_json::Error),

    /// 工厂方法执行失败
    #[error("工厂方法执行失败: {0}")]
    Factory(String),
}

impl From<CacheError> for HybridCacheError {
    /// 将 L2 缓存错误转换为混合缓存错误
    fn from(error: CacheError) -> Self {
        Self::L2(error.to_string())
    }
}
