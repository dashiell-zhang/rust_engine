use application_model::log::models::LogDto;
use application_model::shared::models::PageListDto;
use utoipa::OpenApi;

/// 日志管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    components(
        schemas(
            PageListDto<LogDto>,
            LogDto,
        )
    ),
    tags(
        (name = "日志管理", description = "系统日志查询接口"),
    )
)]
pub struct LogApiDoc;
