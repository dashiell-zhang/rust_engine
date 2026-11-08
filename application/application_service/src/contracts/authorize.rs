use application_model::authorize::models::{
    FunctionRouteDto, FunctionTreeDto, GetFunctionRouteListRequest, GetTokenBySmsRequest,
    GetTokenByWxAppRequest, GetTokenByWxMiniAppRequest, GetTokenRequest, SendSmsVerifyCodeRequest,
    SetFunctionRouteDto, UpdatePasswordByOldPasswordRequest, UpdatePasswordBySmsRequest,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;
use std::collections::HashMap;

#[async_trait]
impl application_interface::services::authorize::AuthorizeService
    for crate::authorize::service::AuthorizeService<'_>
{
    fn get_public_key(&self) -> Result<Option<String>, AppError> {
        crate::authorize::service::AuthorizeService::get_public_key(self)
    }

    async fn init_data(
        &self,
        seed: application_model::authorize::seed::AuthorizeSeedData,
    ) -> Result<application_model::authorize::seed::AuthorizeSeedResult, AppError> {
        crate::authorize::service::AuthorizeService::init_data(self, seed).await
    }

    async fn get_token(&self, request: GetTokenRequest) -> Result<String, AppError> {
        crate::authorize::service::AuthorizeService::get_token(self, request).await
    }
    async fn get_token_by_sms(&self, request: GetTokenBySmsRequest) -> Result<String, AppError> {
        crate::authorize::service::AuthorizeService::get_token_by_sms(self, request).await
    }
    async fn get_token_by_wx_mini_app(
        &self,
        request: GetTokenByWxMiniAppRequest,
    ) -> Result<String, AppError> {
        crate::authorize::service::AuthorizeService::get_token_by_wx_mini_app(self, request).await
    }
    async fn get_token_by_wx_app(
        &self,
        request: GetTokenByWxAppRequest,
    ) -> Result<String, AppError> {
        crate::authorize::service::AuthorizeService::get_token_by_wx_app(self, request).await
    }
    async fn send_sms_verify_code(
        &self,
        request: SendSmsVerifyCodeRequest,
    ) -> Result<bool, AppError> {
        crate::authorize::service::AuthorizeService::send_sms_verify_code(self, request).await
    }
    async fn update_password_by_sms(
        &self,
        current_user: CurrentUser,
        request: UpdatePasswordBySmsRequest,
    ) -> Result<bool, AppError> {
        crate::authorize::service::AuthorizeService::update_password_by_sms(
            self,
            current_user,
            request,
        )
        .await
    }
    async fn update_password_by_old_password(
        &self,
        current_user: CurrentUser,
        request: UpdatePasswordByOldPasswordRequest,
    ) -> Result<bool, AppError> {
        crate::authorize::service::AuthorizeService::update_password_by_old_password(
            self,
            current_user,
            request,
        )
        .await
    }
    async fn try_renew_token(
        &self,
        user_id: i64,
        token_id: i64,
        nbf: i64,
        exp: i64,
    ) -> Result<Option<String>, AppError> {
        crate::authorize::service::AuthorizeService::try_renew_token(
            self, user_id, token_id, nbf, exp,
        )
        .await
    }
    async fn check_function_authorize(
        &self,
        current_user: CurrentUser,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<bool, AppError> {
        crate::authorize::service::AuthorizeService::check_function_authorize(
            self,
            current_user,
            module,
            route,
            allow_unbound,
        )
        .await
    }
    async fn get_function_list(
        &self,
        current_user: CurrentUser,
        sign: Option<&str>,
    ) -> Result<HashMap<String, String>, AppError> {
        crate::authorize::service::AuthorizeService::get_function_list(self, current_user, sign)
            .await
    }
    async fn get_function_route_list(
        &self,
        request: GetFunctionRouteListRequest,
    ) -> Result<Vec<FunctionRouteDto>, AppError> {
        crate::authorize::service::AuthorizeService::get_function_route_list(self, request).await
    }
    async fn set_function_route(&self, request: SetFunctionRouteDto) -> Result<bool, AppError> {
        crate::authorize::service::AuthorizeService::set_function_route(self, request).await
    }
    async fn get_function_tree(&self) -> Result<Vec<FunctionTreeDto>, AppError> {
        crate::authorize::service::AuthorizeService::get_function_tree(self).await
    }
}
