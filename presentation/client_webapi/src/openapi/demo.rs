use utoipa::OpenApi;

use crate::handlers::demo;

/// 客户端服务与 LLM 演示文档。
#[derive(OpenApi)]
#[openapi(
    paths(
        demo::echo,
        demo::add,
        demo::echo2,
        demo::add2,
        demo::a_ref,
        demo::stream_numbers,
        demo::stream_numbers2,
        demo::test_llm,
        demo::test_llm_stream,
    ),
    tags((name = "Demo", description = "服务代理、异步序列与 LLM 调用"))
)]
pub struct DemoApiDoc;
