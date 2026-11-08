use application_model::authorize::models::{
    EcdsaKeyPairDto, GetFunctionListRequest, GetTokenRequest,
};
use application_model::authorize::seed::{AuthorizeInitDataResult, AuthorizeSeedResult};
use utoipa::OpenApi;

use crate::handlers::authorize;

/// 授权模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        authorize::get_token,
        authorize::init_data,
        authorize::get_function_list,
        authorize::update_route,
        authorize::generate_ecdsa_key_pair,
    ),
    components(
        schemas(
            GetTokenRequest,
            GetFunctionListRequest,
            EcdsaKeyPairDto,
            AuthorizeInitDataResult,
            AuthorizeSeedResult,
        )
    ),
    tags(
        (name = "授权", description = "登录与授权接口"),
    )
)]
pub struct AuthorizeApiDoc;
