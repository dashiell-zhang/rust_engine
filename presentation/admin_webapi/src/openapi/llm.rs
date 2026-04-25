use application_model::llm::models::{
    EditLlmAppDto, LlmAppDto, LlmAppPageListDto, LlmConversationDto, LlmConversationPageListDto,
    TestLlmAppRequest, TestLlmAppResult, TestLlmAppUsage,
};
use utoipa::OpenApi;

use crate::handlers::llm;

/// LLM 管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        llm::get_llm_app_list,
        llm::create_llm_app,
        llm::update_llm_app,
        llm::delete_llm_app,
        llm::test_llm_app,
        llm::get_llm_conversation_list,
    ),
    components(
        schemas(
            LlmAppPageListDto,
            LlmAppDto,
            EditLlmAppDto,
            TestLlmAppRequest,
            TestLlmAppResult,
            TestLlmAppUsage,
            LlmConversationPageListDto,
            LlmConversationDto,
        )
    ),
    tags(
        (name = "LLM 管理", description = "LLM 应用配置与调用测试接口"),
    )
)]
pub struct LlmApiDoc;
