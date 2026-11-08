use application_model::llm::models::{
    EditLlmAppDto, EditLlmModelDto, LlmAppDto, LlmConversationDto, LlmModelDto, LlmModelSelectDto,
    TestLlmAppRequest, TestLlmAppResult, TestLlmAppUsage,
};
use application_model::shared::models::PageListDto;
use utoipa::OpenApi;

use crate::handlers::llm;

/// LLM 管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        llm::get_llm_model_list,
        llm::get_llm_model_select,
        llm::create_llm_model,
        llm::update_llm_model,
        llm::delete_llm_model,
        llm::get_llm_app_list,
        llm::create_llm_app,
        llm::update_llm_app,
        llm::delete_llm_app,
        llm::test_llm_app,
        llm::get_llm_conversation_list,
    ),
    components(
        schemas(
            PageListDto<LlmAppDto>,
            LlmAppDto,
            EditLlmAppDto,
            PageListDto<LlmModelDto>,
            LlmModelDto,
            LlmModelSelectDto,
            EditLlmModelDto,
            TestLlmAppRequest,
            TestLlmAppResult,
            TestLlmAppUsage,
            PageListDto<LlmConversationDto>,
            LlmConversationDto,
        )
    ),
    tags(
        (name = "LLM 管理", description = "LLM 应用配置与调用测试接口"),
    )
)]
pub struct LlmApiDoc;
