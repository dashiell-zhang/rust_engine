use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 支付模块请求与返回模型
///
/// 定义客户端支付接口所需的查询参数与回调数据结构
/// 支付订单查询参数
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentOrderQuery {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,
}

/// 支付宝 H5 支付查询参数
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
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
#[serde(rename_all = "camelCase")]
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
    #[serde(rename = "timeStamp")]
    pub time_stamp: i64,

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
    #[serde(rename = "timeStamp")]
    pub time_stamp: i64,

    /// 签名
    pub sign: String,
}

/// 微信支付 H5 发起参数
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateWxPayH5Query {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,

    /// 客户端 IP 地址
    #[serde(rename = "clientIp")]
    pub client_ip: String,
}

/// 微信支付退款请求
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayRefundRequest {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,

    /// 商户退款单号
    #[serde(rename = "outRefundNo")]
    pub out_refund_no: String,

    /// 退款金额，单位元，例如 0.01
    #[serde(rename = "refundAmount")]
    pub refund_amount: String,
}

/// 微信支付退款查询参数
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayRefundQuery {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,

    /// 商户退款单号
    #[serde(rename = "outRefundNo")]
    pub out_refund_no: String,
}

/// 微信支付退款金额信息
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayRefundAmountDto {
    /// 币种
    pub currency: String,

    /// 退款金额，单位分
    pub refund: i32,

    /// 原订单金额，单位分
    pub total: i32,
}

/// 微信支付退款结果
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayRefundResultDto {
    /// 商户退款单号
    #[serde(rename = "outRefundNo")]
    pub out_refund_no: String,

    /// 商户订单号
    #[serde(rename = "outTradeNo")]
    pub out_trade_no: String,

    /// 微信退款单号
    #[serde(rename = "refundId")]
    pub refund_id: String,

    /// 退款状态
    pub status: String,

    /// 微信支付流水号
    #[serde(rename = "transactionId", skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,

    /// 退款成功时间
    #[serde(rename = "successTime", skip_serializing_if = "Option::is_none")]
    pub success_time: Option<String>,

    /// 金额信息
    pub amount: WxPayRefundAmountDto,
}

/// 支付宝退款请求
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AliPayRefundRequest {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,

    /// 退款金额，单位元，例如 0.01
    #[serde(rename = "refundAmount")]
    pub refund_amount: String,

    /// 退款请求号，用于幂等与查询
    #[serde(rename = "outRequestNo")]
    pub out_request_no: String,
}

/// 支付宝退款查询参数
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AliPayRefundQuery {
    /// 订单号
    #[serde(rename = "orderNo")]
    pub order_no: String,

    /// 退款请求号
    #[serde(rename = "outRequestNo")]
    pub out_request_no: String,
}

/// 支付宝退款结果
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AliPayRefundResultDto {
    /// 支付宝交易号
    #[serde(rename = "tradeNo", skip_serializing_if = "Option::is_none")]
    pub trade_no: Option<String>,

    /// 商户订单号
    #[serde(rename = "outTradeNo", skip_serializing_if = "Option::is_none")]
    pub out_trade_no: Option<String>,

    /// 退款请求号
    #[serde(rename = "outRequestNo", skip_serializing_if = "Option::is_none")]
    pub out_request_no: Option<String>,

    /// 退款金额
    #[serde(rename = "refundAmount", skip_serializing_if = "Option::is_none")]
    pub refund_amount: Option<String>,

    /// 原订单金额
    #[serde(rename = "totalAmount", skip_serializing_if = "Option::is_none")]
    pub total_amount: Option<String>,

    /// 本次请求是否发生资金变动
    #[serde(rename = "fundChange", skip_serializing_if = "Option::is_none")]
    pub fund_change: Option<String>,

    /// 退款状态
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// 微信支付回调资源加密体
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayNotifyResource {
    /// 原始数据类型
    pub original_type: Option<String>,

    /// 加密算法
    pub algorithm: String,

    /// 加密后的密文
    pub ciphertext: String,

    /// 附加数据
    pub associated_data: Option<String>,

    /// 随机串
    pub nonce: String,
}

/// 微信支付异步回调入参
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayNotifyDto {
    /// 通知 ID
    pub id: Option<String>,

    /// 通知类型
    pub event_type: Option<String>,

    /// 资源信息
    pub resource: WxPayNotifyResource,
}

/// 微信支付回调响应体
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayNotifyRetDto {
    /// 处理结果码，成功为 SUCCESS，失败为 FAIL
    pub code: String,

    /// 错误描述，成功时为空
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
