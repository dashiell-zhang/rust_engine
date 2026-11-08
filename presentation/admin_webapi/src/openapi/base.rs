use utoipa::OpenApi;

/// 基础工具模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    tags(
        (name = "基础工具", description = "地区、基础数据与工具接口"),
    )
)]
pub struct BaseApiDoc;
