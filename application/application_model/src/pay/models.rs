use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 支付模块请求与返回模型
///
/// 定义客户端支付接口所需的查询参数与回调数据结构
/// 支付订单查询参数
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct PaymentOrderQuery {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,
}

/// 支付宝 H5 支付查询参数
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct CreateAliPayH5Query {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,

    /// 支付完成同步返回地址
    #[serde(rename = "returnUrl")]
    pub return_url: Option<String>,

    /// 用户主动退出支付页后的跳转地址
    #[serde(rename = "quitUrl")]
    pub quit_url: Option<String>,
}

/// 支付宝异步通知结果
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AliPayNotifyResultDto {
    /// 是否已成功处理回调
    #[serde(rename = "isSuccess")]
    pub is_success: bool,

    /// 返回给调用方的结果文本
    pub message: String,
}

/// 微信支付 JSAPI 发起参数
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct CreateWxPayJsapiQuery {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,

    /// 用户 OpenId
    #[serde(rename = "openId")]
    pub open_id: String,
}

/// 微信支付 JSAPI 返回结果
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayJsapiDto {
    /// 应用标识
    #[serde(rename = "appId")]
    pub app_id: String,

    /// 订单详情扩展字符串，格式固定为 prepay_id=xxx
    pub package: String,

    /// 随机字符串
    #[serde(rename = "nonceStr")]
    pub nonce_str: String,

    /// Unix 时间戳（秒）
    #[serde(rename = "timeStamp", deserialize_with = "crate::shared::input::id")]
    pub time_stamp: String,

    /// 签名
    pub sign: String,
}

/// 微信支付 App 返回结果
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayAppDto {
    /// 应用标识
    #[serde(rename = "appId")]
    pub app_id: String,

    /// 商户号
    #[serde(rename = "partnerId")]
    pub partner_id: String,

    /// 预支付交易会话 ID
    #[serde(rename = "prepayId")]
    pub prepay_id: String,

    /// 随机字符串
    #[serde(rename = "nonceStr")]
    pub nonce_str: String,

    /// Unix 时间戳（秒）
    #[serde(rename = "timeStamp", deserialize_with = "crate::shared::input::id")]
    pub time_stamp: String,

    /// 签名
    pub sign: String,
}

/// 微信支付 H5 发起参数
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct CreateWxPayH5Query {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,

    /// 客户端 IP 地址
    #[serde(rename = "clientIp")]
    pub client_ip: String,
}
