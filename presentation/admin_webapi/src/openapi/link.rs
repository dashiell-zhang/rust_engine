use application_model::link::models::{EditLinkDto, LinkDto};
use application_model::shared::models::PageListDto;
use utoipa::OpenApi;

/// 友情链接模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    components(
        schemas(
            PageListDto<LinkDto>,
            LinkDto,
            EditLinkDto,
        )
    ),
    tags(
        (name = "友情链接", description = "友情链接管理接口"),
    )
)]
pub struct LinkApiDoc;
