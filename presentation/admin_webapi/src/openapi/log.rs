use application_model::log::models::{LogDto, LogPageListDto};
use utoipa::OpenApi;

use crate::handlers::log;

/// 日志管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        log::get_log_list,
    ),
    components(
        schemas(
            LogPageListDto,
            LogDto,
        )
    ),
    tags(
        (name = "日志管理", description = "系统日志查询接口"),
    )
)]
pub struct LogApiDoc;
