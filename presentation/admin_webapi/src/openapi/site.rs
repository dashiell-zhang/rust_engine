use application_model::site::models::{EditSiteDto, ServerInfoDto, SiteDto};
use utoipa::OpenApi;

use crate::handlers::site;

/// 站点管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    // 这里显式列出接口和模型 让站点模块文档可以独立拼装
    paths(
        site::get_server_info,
        site::get_site,
        site::update_site,
    ),
    components(
        schemas(
            ServerInfoDto,
            SiteDto,
            EditSiteDto,
        )
    ),
    tags(
        (name = "站点管理", description = "站点基础信息管理接口"),
    )
)]
pub struct SiteApiDoc;
