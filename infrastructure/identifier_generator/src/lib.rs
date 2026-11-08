/// 雪花标识生成器 crate
///
/// 提供基于 Snowflake 算法的分布式有序唯一 ID 生成能力
/// 自动模式通过共享 Redis 原子占用节点并立即续期，失去有效租约时停止发号。
/// 雪花标识生成器核心实现
mod generator;
pub mod layout;
mod lease;
mod settings;

pub use generator::{IdentifierGenerator, IdentifierGeneratorError};
pub use settings::IdentifierGeneratorSettings;
