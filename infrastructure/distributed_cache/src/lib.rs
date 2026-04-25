/// 分布式缓存抽象 crate
///
/// 定义统一的缓存操作接口与公共类型
/// 具体实现由独立 crate 提供：
/// - `distributed_cache_memory`：基于进程内内存，适用于单进程场景
/// - `distributed_cache_redis`：基于 Redis，适用于跨进程与跨机器场景
mod cache;
mod error;
mod extensions;
pub mod options;

pub use cache::DistributedCache;
pub use error::CacheError;
pub use extensions::DistributedCacheExt;
pub use options::{CacheEntryOptions, CacheExpiry};
