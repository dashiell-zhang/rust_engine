use application_model::llm::models::{
    EditLlmModelDto, LlmModelPageListDto, LlmModelPageRequest, LlmModelSelectDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

/// LLM 模型配置应用契约。
#[async_trait]
pub trait LlmModelService: Send + Sync {
    async fn get_llm_model_list(
        &self,
        params: LlmModelPageRequest,
    ) -> Result<LlmModelPageListDto, AppError>;
    async fn get_llm_model_select(&self) -> Result<Vec<LlmModelSelectDto>, AppError>;
    async fn create_llm_model(
        &self,
        dto: EditLlmModelDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>;
    async fn update_llm_model(
        &self,
        id: i64,
        dto: EditLlmModelDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn delete_llm_model(&self, id: i64, current_user: &CurrentUser)
    -> Result<bool, AppError>;
}
