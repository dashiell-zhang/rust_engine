use utoipa::OpenApi;

/// 基础工具模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    tags(
        (name = "基础工具", description = "基础数据、地区、验证码与工具接口"),
    )
)]
pub struct BaseApiDoc;
