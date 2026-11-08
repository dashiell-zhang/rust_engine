use application_model::llm::models::{
    EditLlmModelDto, LlmModelPageListDto, LlmModelPageRequest, LlmModelSelectDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::llm_model::LlmModelService
    for crate::llm::model_service::LlmModelService<'_>
{
    async fn get_llm_model_list(
        &self,
        params: LlmModelPageRequest,
    ) -> Result<LlmModelPageListDto, AppError> {
        crate::llm::model_service::LlmModelService::get_llm_model_list(self, params).await
    }
    async fn get_llm_model_select(&self) -> Result<Vec<LlmModelSelectDto>, AppError> {
        crate::llm::model_service::LlmModelService::get_llm_model_select(self).await
    }
    async fn create_llm_model(
        &self,
        dto: EditLlmModelDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        crate::llm::model_service::LlmModelService::create_llm_model(self, dto, current_user).await
    }
    async fn update_llm_model(
        &self,
        id: i64,
        dto: EditLlmModelDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::llm::model_service::LlmModelService::update_llm_model(self, id, dto, current_user)
            .await
    }
    async fn delete_llm_model(
        &self,
        id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::llm::model_service::LlmModelService::delete_llm_model(self, id, current_user).await
    }
}
