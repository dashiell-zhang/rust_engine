use application_model::authorize::models::{
    GetFunctionListRequest, GetTokenBySmsRequest, GetTokenByWxAppRequest,
    GetTokenByWxMiniAppRequest, GetTokenRequest, SendSmsVerifyCodeRequest,
    UpdatePasswordByOldPasswordRequest, UpdatePasswordBySmsRequest,
};
use utoipa::OpenApi;

/// 授权模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    // 客户端授权文档覆盖登录 验证码和功能列表接口
    components(
        schemas(
            GetTokenRequest,
            GetTokenBySmsRequest,
            GetTokenByWxMiniAppRequest,
            GetTokenByWxAppRequest,
            GetFunctionListRequest,
            SendSmsVerifyCodeRequest,
            UpdatePasswordBySmsRequest,
            UpdatePasswordByOldPasswordRequest,
        )
    ),
    tags(
        (name = "授权", description = "登录与授权接口"),
    )
)]
pub struct AuthorizeApiDoc;
