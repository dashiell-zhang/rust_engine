use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;
use webapi_core::{
    builder::protect_method_router,
    cache_data::{CacheDataOptions, cache_method_router_with_options},
    queue_limit::{QueueLimitOptions, queue_limit_method_router},
};

use crate::{
    handlers::authorize::{
        get_function_list, get_public_key, get_token, get_token_by_sms, get_token_by_wx_app,
        get_token_by_wx_mini_app, send_sms_verify_code, update_password_by_old_password,
        update_password_by_sms, update_route,
    },
    openapi::authorize::AuthorizeApiDoc,
    state::AppState,
};

use super::{WebApiFeature, WebApiRoutePermission};

/// 授权特性
pub struct AuthorizeFeature;

impl WebApiFeature for AuthorizeFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        // 登录和验证码发送接口允许匿名访问 其余接口走统一权限保护
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/Authorize/UpdateRoute",
                webapi_core::routes::get(update_route),
            )
            .route(
                "/Authorize/GetPublicKey",
                webapi_core::routes::get(get_public_key),
            )
            .route(
                "/Authorize/GetToken",
                webapi_core::routes::post(get_token).json_body("login"),
            )
            .route(
                "/Authorize/GetTokenBySMS",
                webapi_core::routes::post(get_token_by_sms).json_body("login"),
            )
            .route(
                "/Authorize/GetTokenByWeiXinMiniApp",
                webapi_core::routes::post(get_token_by_wx_mini_app).json_body("login"),
            )
            .route(
                "/Authorize/GetTokenByWeiXinApp",
                webapi_core::routes::post(get_token_by_wx_app).json_body("login"),
            )
            .route(
                "/Authorize/SendSMSVerifyCode",
                webapi_core::routes::post(send_sms_verify_code).json_body("sendVerifyCode"),
            )
            .route(
                "/Authorize/GetFunctionList",
                protect_method_router(
                    cache_method_router_with_options(
                        webapi_core::routes::get(get_function_list),
                        app_state.clone(),
                        CacheDataOptions::with_authorization(60),
                        concat!(module_path!(), "::get_function_list"),
                    ),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Authorize/GetFunctionList",
                    false,
                ),
            )
            .route(
                "/Authorize/UpdatePasswordBySMS",
                protect_method_router(
                    queue_limit_method_router(
                        webapi_core::routes::post(update_password_by_sms)
                            .json_body("updatePassword"),
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::rejecting_with_authorization(1),
                        concat!(module_path!(), "::update_password_by_sms"),
                    ),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Authorize/UpdatePasswordBySMS",
                    false,
                ),
            )
            .route(
                "/Authorize/UpdatePasswordByOldPassword",
                protect_method_router(
                    queue_limit_method_router(
                        webapi_core::routes::post(update_password_by_old_password)
                            .json_body("updatePassword"),
                        app_state.distributed_lock.clone(),
                        QueueLimitOptions::rejecting_with_authorization(1),
                        concat!(module_path!(), "::update_password_by_old_password"),
                    ),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Authorize/UpdatePasswordByOldPassword",
                    false,
                ),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(AuthorizeApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        // 只登记需要授权校验的接口 匿名接口不进入权限清单
        vec![
            WebApiRoutePermission {
                route: "/Authorize/GetFunctionList",
                remark: "获取授权功能列表",
            },
            WebApiRoutePermission {
                route: "/Authorize/UpdatePasswordBySMS",
                remark: "通过短信验证码修改账户密码",
            },
            WebApiRoutePermission {
                route: "/Authorize/UpdatePasswordByOldPassword",
                remark: "通过老密码修改密码",
            },
        ]
    }
}
