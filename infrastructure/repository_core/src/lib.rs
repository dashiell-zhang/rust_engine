//! 与业务实体无关的数据库公共能力。
pub mod database_health;
pub mod extensions;
pub mod json_index;
pub mod migration;
pub mod pagination;
pub mod partition;
pub mod postgres_operation;
pub mod read_database;
pub mod versioned;

pub use extensions::{SoftDeleteEntityExt, SoftDeleteQueryExt};
pub use read_database::ReadDatabaseConnection;
pub use source_generator_core::model_documentation;
