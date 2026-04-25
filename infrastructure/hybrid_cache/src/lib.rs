/// L1+L2 混合缓存 crate
///
/// 提供进程内内存缓存（L1）与外部分布式缓存（L2）的统一封装
/// L1 基于 moka，L2 对接实现了 `DistributedCache` 的任意后端
/// 典型用法：热点数据优先从 L1 毫秒级读取，冷数据回落至 L2，缓存失效时同步清除两层
mod cache;
mod error;

pub use cache::HybridCache;
pub use error::HybridCacheError;
