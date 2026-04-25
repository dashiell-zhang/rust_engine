use application_model::pay::models::{
    CreateWxPayJsapiQuery, PaymentOrderQuery, WxPayAppDto, WxPayJsapiDto, WxPayNotifyDto,
    WxPayNotifyRetDto,
};
use utoipa::OpenApi;

use crate::handlers::pay;

/// 支付模块开放接口文档
///
/// 聚合客户端支付发起与回调接口对应的请求响应模型
#[derive(OpenApi)]
#[openapi(
    paths(
        pay::create_alipay_pc,
        pay::create_alipay_mini_app,
        pay::create_alipay_h5,
        pay::alipay_notify,
        pay::create_wx_pay_jsapi,
        pay::create_wx_pay_app,
        pay::create_wx_pay_h5,
        pay::create_wx_pay_pc,
        pay::wx_pay_notify,
    ),
    components(
        schemas(
            PaymentOrderQuery,
            CreateWxPayJsapiQuery,
            WxPayJsapiDto,
            WxPayAppDto,
            WxPayNotifyDto,
            WxPayNotifyRetDto,
        )
    ),
    tags(
        (name = "支付", description = "客户端支付发起与回调接口"),
    )
)]
pub struct PayApiDoc;
