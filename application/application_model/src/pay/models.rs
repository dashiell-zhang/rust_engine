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

/// 微信退款响应金额，单位为分；缺失的整数遵循源模型零值。
#[derive(Debug, Default, Deserialize, Serialize, ToSchema)]
#[serde(default)]
pub struct WxPayRefundAmountDto {
    /// 币种
    pub currency: Option<String>,
    /// 优惠退款金额
    pub discount_refund: i32,
    /// 用户退款金额
    pub payer_refund: i32,
    /// 用户支付金额
    pub payer_total: i32,
    /// 退款金额
    pub refund: i32,
    /// 手续费退款金额
    pub refund_fee: i32,
    /// 应结退款金额
    pub settlement_refund: i32,
    /// 应结订单金额
    pub settlement_total: i32,
    /// 原订单金额
    pub total: i32,
}

/// 微信退款协议响应，保留提供商字段名与源模型的可空引用字段。
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct WxPayRefundResultDto {
    /// 退款金额明细
    pub amount: Option<WxPayRefundAmountDto>,
    /// 退款渠道
    pub channel: Option<String>,
    /// 退款创建时间
    #[serde(
        default = "crate::shared::input::default_date_time",
        deserialize_with = "common::json_helper::date_time::deserialize_offset_string"
    )]
    pub create_time: String,
    /// 资金账户
    pub funds_account: Option<String>,
    /// 商户退款单号
    pub out_refund_no: Option<String>,
    /// 商户订单号
    pub out_trade_no: Option<String>,
    /// 微信退款单号
    pub refund_id: Option<String>,
    /// 退款状态
    pub status: Option<String>,
    /// 退款完成时间
    #[serde(
        default = "crate::shared::input::default_date_time",
        deserialize_with = "common::json_helper::date_time::deserialize_offset_string"
    )]
    pub success_time: String,
    /// 微信支付流水号
    pub transaction_id: Option<String>,
    /// 用户收款账户
    pub user_received_account: Option<String>,
}

/// 微信支付回调资源加密体
#[derive(Debug, Deserialize, ToSchema, common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.Pay.WeiXinPayNotifyDto+Resource")]
pub struct WxPayNotifyResource {
    /// 原始数据类型
    #[json(name = "original_type")]
    pub original_type: String,

    /// 加密算法
    #[json(name = "algorithm")]
    pub algorithm: String,

    /// 加密后的密文
    #[json(name = "ciphertext")]
    pub ciphertext: String,

    /// 附加数据
    #[json(name = "associated_data")]
    pub associated_data: String,

    /// 随机串
    #[json(name = "nonce")]
    pub nonce: String,
}

/// 微信支付异步回调入参
#[derive(Debug, Deserialize, ToSchema, common::json_helper::input::JsonModel)]
#[json(type_name = "Application.Model.Pay.WeiXinPayNotifyDto")]
pub struct WxPayNotifyDto {
    /// 通知 ID
    #[json(name = "id")]
    pub id: String,

    /// 通知创建时间
    #[serde(
        default = "crate::shared::input::default_date_time",
        deserialize_with = "crate::shared::input::date_time"
    )]
    #[json(name = "create_time", value, scalar_type = "System.DateTimeOffset")]
    pub create_time: String,

    /// 资源类型
    #[json(name = "resource_type")]
    pub resource_type: String,

    /// 通知类型
    #[json(name = "event_type")]
    pub event_type: String,

    /// 通知摘要
    #[json(name = "summary")]
    pub summary: String,

    /// 资源信息
    #[json(nested)]
    #[json(name = "resource")]
    pub resource: WxPayNotifyResource,
}

/// 微信支付回调响应体
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WxPayNotifyRetDto {
    /// 处理结果码，成功为 SUCCESS，失败为 FAIL
    pub code: String,

    /// 错误描述，成功时为空
    pub message: Option<String>,
}

/// 微信支付通知解密后的交易信息；内部反序列化不施加 MVC 必填验证。
#[derive(Debug, Deserialize)]
pub struct WxPayNotifyTransactionDto {
    /// 通知中的商户号
    pub mchid: Option<String>,
    /// 通知中的应用标识
    pub appid: Option<String>,
    /// 商户订单号
    pub out_trade_no: Option<String>,
    /// 微信支付流水号
    pub transaction_id: Option<String>,
    /// 交易类型
    pub trade_type: Option<String>,
    /// 交易状态
    pub trade_state: Option<String>,
    /// 交易状态描述
    pub trade_state_desc: Option<String>,
    /// 付款银行
    pub bank_type: Option<String>,
    /// 商户附加数据
    pub attach: Option<String>,
    /// 支付完成时间，缺失时保留最小日期
    #[serde(
        default = "crate::shared::input::default_date_time",
        deserialize_with = "common::json_helper::date_time::deserialize_offset_string"
    )]
    pub success_time: String,
    /// 付款人信息
    pub payer: Option<WxPayNotifyPayer>,
    /// 支付金额
    pub amount: Option<WxPayNotifyTransactionAmount>,
}

/// 通知中的付款人。
#[derive(Debug, Deserialize)]
pub struct WxPayNotifyPayer {
    /// 付款人在对应应用中的标识
    pub openid: Option<String>,
}

/// 微信交易通知金额，单位为分。
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WxPayNotifyTransactionAmount {
    /// 订单金额
    pub total: i32,
    /// 用户支付金额
    pub payer_total: i32,
    /// 订单币种
    pub currency: Option<String>,
    /// 用户支付币种
    pub payer_currency: Option<String>,
}

/// 微信退款通知解密后的信息。
#[derive(Debug, Deserialize)]
pub struct WxPayNotifyRefundDto {
    /// 商户号
    pub mchid: Option<String>,
    /// 商户订单号
    pub out_trade_no: Option<String>,
    /// 微信支付流水号
    pub transaction_id: Option<String>,
    /// 商户退款单号
    pub out_refund_no: Option<String>,
    /// 微信退款单号
    pub refund_id: Option<String>,
    /// 退款状态
    pub refund_status: Option<String>,
    /// 退款完成时间
    #[serde(
        default = "crate::shared::input::default_date_time",
        deserialize_with = "common::json_helper::date_time::deserialize_offset_string"
    )]
    pub success_time: String,
    /// 退款金额
    pub amount: Option<WxPayNotifyRefundAmount>,
    /// 用户收款账户
    pub user_received_account: Option<String>,
}

/// 微信退款通知金额，单位为分。
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WxPayNotifyRefundAmount {
    /// 原订单金额
    pub total: i32,
    /// 退款金额
    pub refund: i32,
    /// 用户支付金额
    pub payer_total: i32,
    /// 用户退款金额
    pub payer_refund: i32,
}
