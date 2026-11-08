use utoipa::OpenApi;

/// 客户端服务与 LLM 演示文档。
#[derive(OpenApi)]
#[openapi(
    tags((name = "Demo", description = "服务代理、异步序列与 LLM 调用"))
)]
pub struct DemoApiDoc;
