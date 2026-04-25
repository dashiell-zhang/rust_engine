use axum::Router;
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::{
    builder::protect_method_router,
    cache_data::{CacheDataOptions, cache_method_router_with_options},
};

use crate::{
    handlers::user::{edit_user_phone_by_sms, get_user},
    openapi::user::UserApiDoc,
    state::AppState,
};

use super::{WebApiFeature, WebApiRoutePermission};

/// 用户特性
pub struct UserFeature;

impl WebApiFeature for UserFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        // 用户端接口全部要求登录后访问
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/User/GetUser",
                protect_method_router(
                    cache_method_router_with_options(
                        axum::routing::get(get_user),
                        app_state.clone(),
                        CacheDataOptions::with_authorization(60),
                    ),
                    app_state.clone(),
                    permission_module.clone(),
                    "/User/GetUser",
                    false,
                ),
            )
            .route(
                "/User/EditUserPhoneBySms",
                protect_method_router(
                    axum::routing::post(edit_user_phone_by_sms),
                    app_state,
                    permission_module,
                    "/User/EditUserPhoneBySms",
                    false,
                ),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(UserApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        // 这里的路由清单会参与权限初始化和运行期校验
        vec![
            WebApiRoutePermission {
                route: "/User/GetUser",
                remark: "获取用户信息",
            },
            WebApiRoutePermission {
                route: "/User/EditUserPhoneBySms",
                remark: "通过短信验证码修改当前用户手机号",
            },
        ]
    }
}
