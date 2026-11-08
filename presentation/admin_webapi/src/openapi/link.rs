use application_model::link::models::{EditLinkDto, LinkDto};
use application_model::shared::models::PageListDto;
use utoipa::OpenApi;

use crate::handlers::link;

/// 友情链接模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        link::get_link_list,
        link::get_link,
        link::create_link,
        link::update_link,
        link::delete_link,
    ),
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
