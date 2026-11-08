use application_model::llm::models::{
    EditLlmAppDto, LlmAppDto, LlmAppPageListDto, LlmAppPageRequest, TestLlmAppRequest,
    TestLlmAppResult,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

/// LLM 应用配置与试运行应用契约。
#[async_trait]
pub trait LlmAppService: Send + Sync {
    async fn get_llm_app_list(
        &self,
        params: LlmAppPageRequest,
    ) -> Result<LlmAppPageListDto, AppError>;
    async fn get_llm_app(&self, llm_app_id: i64) -> Result<Option<LlmAppDto>, AppError>;
    async fn create_llm_app(
        &self,
        dto: EditLlmAppDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>;
    async fn update_llm_app(
        &self,
        llm_app_id: i64,
        dto: EditLlmAppDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn delete_llm_app(
        &self,
        llm_app_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>;
    async fn test_llm_app(
        &self,
        actor_user_id: i64,
        request: TestLlmAppRequest,
    ) -> Result<TestLlmAppResult, AppError>;

    /// 管理端试用的取消覆盖模型读取、模板处理和提供商调用。
    async fn test_llm_app_with_cancellation(
        &self,
        actor_user_id: i64,
        request: TestLlmAppRequest,
        token: common::cancellation::CancellationToken,
    ) -> Result<TestLlmAppResult, AppError> {
        common::cancellation::run(&token, self.test_llm_app(actor_user_id, request)).await
    }
}
