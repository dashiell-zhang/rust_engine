//! 迁移执行只读取已登记的固定 SQL，不再读取当前实体或索引声明。
use sea_orm::DbBackend;

pub fn initial_schema_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202604050001.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn legacy_alignment_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202604050002.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn llm_model_alignment_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202606240001.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn queue_component_alignment_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202611070001.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn user_role_alignment_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202611070002.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn user_identity_alignment_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202611070003.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn business_model_alignment_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202611070004.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn initial_schema_down_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202604050001.down.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn legacy_alignment_down_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202604050002.down.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn llm_model_alignment_down_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202606240001.down.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn queue_component_alignment_down_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202611070001.down.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn user_role_alignment_down_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202611070002.down.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn user_identity_alignment_down_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202611070003.down.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}

pub fn business_model_alignment_down_statements(_backend: DbBackend) -> Vec<String> {
    let statements: &[&str] = include!("202611070004.down.rs");
    statements.iter().map(|sql| (*sql).to_owned()).collect()
}
