use application_model::llm::models::{
    EditLlmAppDto, EditLlmModelDto, LlmAppDto, LlmConversationDto, LlmModelDto, LlmModelSelectDto,
    TestLlmAppRequest, TestLlmAppResult, TestLlmAppUsage,
};
use application_model::shared::models::PageListDto;
use utoipa::OpenApi;

/// LLM 管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
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
