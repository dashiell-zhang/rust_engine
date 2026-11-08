//! 默认模型配置来源：复用宿主业务写池，不维护额外缓存。
use async_trait::async_trait;
use common::app_error::AppError;
use llm::{LlmModelConfig, LlmModelConfigResolver};
use repository::extensions::SoftDeleteQueryExt;
use repository::llm_model;
use sea_orm::{ColumnTrait, DatabaseConnection, QueryFilter};
use std::sync::Arc;

pub struct DatabaseLlmModelConfigResolver {
    db: Arc<DatabaseConnection>,
}
impl DatabaseLlmModelConfigResolver {
    pub fn new(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }
}
#[async_trait]
impl LlmModelConfigResolver for DatabaseLlmModelConfigResolver {
    async fn get_config(&self, model_id: i64) -> Result<Option<LlmModelConfig>, AppError> {
        let model = llm_model::Entity::find()
            .filter(llm_model::Column::Id.eq(model_id))
            .filter(llm_model::Column::IsEnable.eq(true))
            .not_deleted(llm_model::Column::DeleteTime)
            .one(self.db.as_ref())
            .await
            .map_err(|error| AppError::internal(error.to_string()))?;

        Ok(model.map(|model| LlmModelConfig {
            endpoint: model.endpoint,
            api_key: model.api_key,
            model_id: model.model_id,
            protocol_type: model.protocol_type,
        }))
    }
}
