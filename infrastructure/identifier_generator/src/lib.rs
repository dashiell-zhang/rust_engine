/// 雪花标识生成器 crate
///
/// 提供基于 Snowflake 算法的分布式有序唯一 ID 生成能力
/// 支持在多节点部署场景下通过 `RefreshSignTask` 动态刷新机器标识，保证跨实例 ID 不重复
/// 雪花标识生成器核心实现
mod generator;
mod settings;

/// 机器标识刷新后台任务
pub mod refresh;

pub use generator::{IdentifierGenerator, IdentifierGeneratorError};
pub use settings::IdentifierGeneratorSettings;
