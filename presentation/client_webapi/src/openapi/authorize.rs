use application_model::authorize::models::{
    GetFunctionListRequest, GetTokenBySmsRequest, GetTokenByWxAppRequest,
    GetTokenByWxMiniAppRequest, GetTokenRequest, SendSmsVerifyCodeRequest,
    UpdatePasswordByOldPasswordRequest, UpdatePasswordBySmsRequest,
};
use utoipa::OpenApi;

use crate::handlers::authorize;

/// 授权模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    // 客户端授权文档覆盖登录 验证码和功能列表接口
    paths(
        authorize::update_route,
        authorize::get_public_key,
        authorize::get_token,
        authorize::get_token_by_sms,
        authorize::get_token_by_wx_mini_app,
        authorize::get_token_by_wx_app,
        authorize::get_function_list,
        authorize::send_sms_verify_code,
        authorize::update_password_by_sms,
        authorize::update_password_by_old_password,
    ),
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
