/// 分布式锁抽象 crate
///
/// 定义统一的锁操作接口与公共类型
/// 具体实现由独立 crate 提供：
/// - `distributed_lock_memory`：基于进程内信号量，适用于单进程场景
/// - `distributed_lock_redis`：基于 Redis，适用于跨进程与跨机器的分布式场景
mod error;
mod guard;
mod lock;

pub use error::LockError;
pub use guard::LockGuard;
pub use lock::DistributedLock;
