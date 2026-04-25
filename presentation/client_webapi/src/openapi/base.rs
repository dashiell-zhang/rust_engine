use utoipa::OpenApi;

use crate::handlers::base;

/// 基础工具模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        base::get_region,
        base::get_region_all,
        base::get_qr_code,
        base::get_verify_code,
        base::get_value_list,
        base::get_snowflake_id,
    ),
    tags(
        (name = "基础工具", description = "基础数据、地区、验证码与工具接口"),
    )
)]
pub struct BaseApiDoc;
