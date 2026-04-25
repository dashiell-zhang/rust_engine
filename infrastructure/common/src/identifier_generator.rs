// 实现已迁移至独立 crate `identifier_generator`，此处保留 re-export 以维持向后兼容
pub use identifier_generator::{
    IdentifierGenerator, IdentifierGeneratorError, IdentifierGeneratorSettings,
};
