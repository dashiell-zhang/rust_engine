use application_model::authorize::models::{GetFunctionListRequest, GetTokenRequest};
use utoipa::OpenApi;

use crate::handlers::authorize;

/// 授权模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        authorize::get_token,
        authorize::get_function_list,
        authorize::update_route,
    ),
    components(
        schemas(
            GetTokenRequest,
            GetFunctionListRequest,
        )
    ),
    tags(
        (name = "授权", description = "登录与授权接口"),
    )
)]
pub struct AuthorizeApiDoc;
