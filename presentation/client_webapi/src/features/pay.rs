use crate::openapi::pay::PayApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 支付特性
///
/// 汇总客户端支付相关路由、权限点与开放文档
pub struct PayFeature;

impl WebApiFeature for PayFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::Pay>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::pay::create_alipay_pc),
            webapi_core::endpoint!(crate::handlers::pay::create_alipay_mini_app),
            webapi_core::endpoint!(crate::handlers::pay::create_alipay_h5),
            webapi_core::endpoint!(crate::handlers::pay::create_wx_pay_jsapi),
            webapi_core::endpoint!(crate::handlers::pay::create_wx_pay_app),
            webapi_core::endpoint!(crate::handlers::pay::create_wx_pay_h5),
            webapi_core::endpoint!(crate::handlers::pay::create_wx_pay_pc),
            webapi_core::endpoint!(crate::handlers::pay::alipay_notify),
            webapi_core::endpoint!(crate::handlers::pay::wx_pay_notify),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(PayApiDoc::openapi())
    }
}
