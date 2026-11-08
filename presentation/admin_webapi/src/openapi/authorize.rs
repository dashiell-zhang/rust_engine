use application_model::authorize::models::{
    EcdsaKeyPairDto, GetFunctionListRequest, GetTokenRequest,
};
use application_model::authorize::seed::{AuthorizeInitDataResult, AuthorizeSeedResult};
use utoipa::OpenApi;

/// 授权模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
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
