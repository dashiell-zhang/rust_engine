use application_model::authorize::models::{
    FunctionRouteDto, FunctionTreeDto, GetFunctionRouteListRequest, GetTokenBySmsRequest,
    GetTokenByWxAppRequest, GetTokenByWxMiniAppRequest, GetTokenRequest, SendSmsVerifyCodeRequest,
    SetFunctionRouteDto, UpdatePasswordByOldPasswordRequest, UpdatePasswordBySmsRequest,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;
use std::collections::HashMap;

/// 认证、令牌与功能权限应用契约。
#[async_trait]
pub trait AuthorizeService: Send + Sync {
    fn get_public_key(&self) -> Result<Option<String>, AppError>;

    async fn init_data(
        &self,
        seed: application_model::authorize::seed::AuthorizeSeedData,
    ) -> Result<application_model::authorize::seed::AuthorizeSeedResult, AppError>;

    async fn get_token(&self, request: GetTokenRequest) -> Result<String, AppError>;
    async fn get_token_by_sms(&self, request: GetTokenBySmsRequest) -> Result<String, AppError>;
    async fn get_token_by_wx_mini_app(
        &self,
        request: GetTokenByWxMiniAppRequest,
    ) -> Result<String, AppError>;
    async fn get_token_by_wx_app(
        &self,
        request: GetTokenByWxAppRequest,
    ) -> Result<String, AppError>;
    async fn send_sms_verify_code(
        &self,
        request: SendSmsVerifyCodeRequest,
    ) -> Result<bool, AppError>;
    async fn update_password_by_sms(
        &self,
        current_user: CurrentUser,
        request: UpdatePasswordBySmsRequest,
    ) -> Result<bool, AppError>;
    async fn update_password_by_old_password(
        &self,
        current_user: CurrentUser,
        request: UpdatePasswordByOldPasswordRequest,
    ) -> Result<bool, AppError>;
    async fn try_renew_token(
        &self,
        user_id: i64,
        token_id: i64,
        nbf: i64,
        exp: i64,
    ) -> Result<Option<String>, AppError>;
    async fn check_function_authorize(
        &self,
        current_user: CurrentUser,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<bool, AppError>;
    async fn get_function_list(
        &self,
        current_user: CurrentUser,
        sign: Option<&str>,
    ) -> Result<HashMap<String, String>, AppError>;
    async fn get_function_route_list(
        &self,
        request: GetFunctionRouteListRequest,
    ) -> Result<Vec<FunctionRouteDto>, AppError>;
    async fn set_function_route(&self, request: SetFunctionRouteDto) -> Result<bool, AppError>;
    async fn get_function_tree(&self) -> Result<Vec<FunctionTreeDto>, AppError>;
}
