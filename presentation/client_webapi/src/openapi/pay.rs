use application_model::pay::models::{
    CreateWxPayJsapiQuery, PaymentOrderQuery, WxPayAppDto, WxPayJsapiDto, WxPayNotifyDto,
    WxPayNotifyRetDto,
};
use utoipa::OpenApi;

/// 支付模块开放接口文档
///
/// 聚合客户端支付发起与回调接口对应的请求响应模型
#[derive(OpenApi)]
#[openapi(
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
