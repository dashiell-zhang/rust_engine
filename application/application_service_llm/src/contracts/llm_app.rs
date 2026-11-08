use application_model::llm::models::{
    EditLlmAppDto, LlmAppDto, LlmAppPageListDto, LlmAppPageRequest, TestLlmAppRequest,
    TestLlmAppResult,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::llm_app::LlmAppService
    for crate::llm::app_service::LlmAppService<'_>
{
    async fn get_llm_app_list(
        &self,
        params: LlmAppPageRequest,
    ) -> Result<LlmAppPageListDto, AppError> {
        crate::llm::app_service::LlmAppService::get_llm_app_list(self, params).await
    }
    async fn get_llm_app(&self, llm_app_id: i64) -> Result<Option<LlmAppDto>, AppError> {
        crate::llm::app_service::LlmAppService::get_llm_app(self, llm_app_id).await
    }
    async fn create_llm_app(
        &self,
        dto: EditLlmAppDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        crate::llm::app_service::LlmAppService::create_llm_app(self, dto, current_user).await
    }
    async fn update_llm_app(
        &self,
        llm_app_id: i64,
        dto: EditLlmAppDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::llm::app_service::LlmAppService::update_llm_app(self, llm_app_id, dto, current_user)
            .await
    }
    async fn delete_llm_app(
        &self,
        llm_app_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        crate::llm::app_service::LlmAppService::delete_llm_app(self, llm_app_id, current_user).await
    }
    async fn test_llm_app(
        &self,
        actor_user_id: i64,
        request: TestLlmAppRequest,
    ) -> Result<TestLlmAppResult, AppError> {
        crate::llm::app_service::LlmAppService::test_llm_app(self, actor_user_id, request).await
    }
}
