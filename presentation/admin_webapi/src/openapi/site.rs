use application_model::site::models::{EditSiteDto, ServerInfoDto, SiteDto};
use utoipa::OpenApi;

/// 站点管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    // 这里显式列出接口和模型 让站点模块文档可以独立拼装
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
