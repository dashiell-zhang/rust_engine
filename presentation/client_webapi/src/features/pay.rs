use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;

use crate::{
    handlers::pay::{
        alipay_notify, create_alipay_h5, create_alipay_mini_app, create_alipay_pc,
        create_wx_pay_app, create_wx_pay_h5, create_wx_pay_jsapi, create_wx_pay_pc, wx_pay_notify,
    },
    openapi::pay::PayApiDoc,
    state::AppState,
};

use super::WebApiFeature;

/// 支付特性
///
/// 汇总客户端支付相关路由、权限点与开放文档
pub struct PayFeature;

impl WebApiFeature for PayFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        _app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        router
            .route(
                "/Pay/CreateAliPayPC",
                webapi_core::routes::get(create_alipay_pc),
            )
            .route(
                "/Pay/CreateAliPayMiniApp",
                webapi_core::routes::get(create_alipay_mini_app),
            )
            .route(
                "/Pay/CreateAliPayH5",
                webapi_core::routes::get(create_alipay_h5),
            )
            .route(
                "/Pay/CreateWeiXinPayJSAPI",
                webapi_core::routes::get(create_wx_pay_jsapi),
            )
            .route(
                "/Pay/CreateWeiXinPayApp",
                webapi_core::routes::get(create_wx_pay_app),
            )
            .route(
                "/Pay/CreateWeiXinPayH5",
                webapi_core::routes::get(create_wx_pay_h5),
            )
            .route(
                "/Pay/CreateWeiXinPayPC",
                webapi_core::routes::get(create_wx_pay_pc),
            )
            .route(
                "/Pay/AliPayNotify",
                webapi_core::routes::post(alipay_notify),
            )
            .route(
                "/Pay/WeiXinPayNotify/{mch_id}",
                webapi_core::routes::post(wx_pay_notify).json_body("weiXinPayNotify"),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(PayApiDoc::openapi())
    }
}
