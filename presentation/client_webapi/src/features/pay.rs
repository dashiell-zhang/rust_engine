use axum::Router;
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::builder::protect_method_router;

use crate::{
    handlers::pay::{
        alipay_notify, create_alipay_h5, create_alipay_mini_app, create_alipay_pc,
        create_wx_pay_app, create_wx_pay_h5, create_wx_pay_jsapi, create_wx_pay_pc, wx_pay_notify,
    },
    openapi::pay::PayApiDoc,
    state::AppState,
};

use super::{WebApiFeature, WebApiRoutePermission};

/// 支付特性
///
/// 汇总客户端支付相关路由、权限点与开放文档
pub struct PayFeature;

impl WebApiFeature for PayFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/Pay/CreateAliPayPC",
                protect_method_router(
                    axum::routing::get(create_alipay_pc),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Pay/CreateAliPayPC",
                    false,
                ),
            )
            .route(
                "/Pay/CreateAliPayMiniApp",
                protect_method_router(
                    axum::routing::get(create_alipay_mini_app),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Pay/CreateAliPayMiniApp",
                    false,
                ),
            )
            .route(
                "/Pay/CreateAliPayH5",
                protect_method_router(
                    axum::routing::get(create_alipay_h5),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Pay/CreateAliPayH5",
                    false,
                ),
            )
            .route(
                "/Pay/CreateWeiXinPayJSAPI",
                protect_method_router(
                    axum::routing::get(create_wx_pay_jsapi),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Pay/CreateWeiXinPayJSAPI",
                    false,
                ),
            )
            .route(
                "/Pay/CreateWeiXinPayApp",
                protect_method_router(
                    axum::routing::get(create_wx_pay_app),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Pay/CreateWeiXinPayApp",
                    false,
                ),
            )
            .route(
                "/Pay/CreateWeiXinPayH5",
                protect_method_router(
                    axum::routing::get(create_wx_pay_h5),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Pay/CreateWeiXinPayH5",
                    false,
                ),
            )
            .route(
                "/Pay/CreateWeiXinPayPC",
                protect_method_router(
                    axum::routing::get(create_wx_pay_pc),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Pay/CreateWeiXinPayPC",
                    false,
                ),
            )
            .route("/Pay/AliPayNotify", axum::routing::post(alipay_notify))
            .route(
                "/Pay/WeiXinPayNotify/{mch_id}",
                axum::routing::post(wx_pay_notify),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(PayApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![
            WebApiRoutePermission {
                route: "/Pay/CreateAliPayPC",
                remark: "创建支付宝 PC 支付地址",
            },
            WebApiRoutePermission {
                route: "/Pay/CreateAliPayMiniApp",
                remark: "创建支付宝小程序支付交易号",
            },
            WebApiRoutePermission {
                route: "/Pay/CreateAliPayH5",
                remark: "创建支付宝 H5 支付地址",
            },
            WebApiRoutePermission {
                route: "/Pay/CreateWeiXinPayJSAPI",
                remark: "创建微信支付 JSAPI 预支付参数",
            },
            WebApiRoutePermission {
                route: "/Pay/CreateWeiXinPayApp",
                remark: "创建微信支付 App 预支付参数",
            },
            WebApiRoutePermission {
                route: "/Pay/CreateWeiXinPayH5",
                remark: "创建微信支付 H5 支付链接",
            },
            WebApiRoutePermission {
                route: "/Pay/CreateWeiXinPayPC",
                remark: "创建微信支付 PC 扫码支付链接",
            },
        ]
    }
}
