use std::{collections::HashMap, sync::Arc};
use webapi_support::WebApiApplicationExt;

use application_model::pay::models::{CreateWxPayJsapiQuery, PaymentOrderQuery, WxPayNotifyDto};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use webapi_core::error::AppError;
use webapi_core::extract::{FormFields, Json, JsonWithBody, Query};
use webapi_core::request_context::RequestContext;
use webapi_core::text_response::Text;

use crate::state::AppState;

/// 支付接口处理器
///
/// 负责参数接收、通知地址组装与应用服务调用
/// 创建支付宝 PC 支付地址
#[utoipa::path(
    get,
    path = "/Pay/CreateAliPayPC",
    params(PaymentOrderQuery),
    responses(
        (status = 200, description = "创建成功", body = String),
        (status = 400, description = "订单或配置无效"),
        (status = 204, description = "订单、状态或创建配置不满足条件"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn create_alipay_pc(
    State(app_state): State<Arc<AppState>>,
    request_context: RequestContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay()?;

    // 支付宝异步通知固定回调到当前宿主公开地址
    let notify_url = format!("{}/Pay/AliPayNotify", request_context.base_url());

    match service
        .create_alipay_pc(&query.order_no, &notify_url, None)
        .await
    {
        Ok(Some(result)) => {
            Ok((StatusCode::OK, webapi_core::text_response::Text(result)).into_response())
        }
        Ok(None) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建支付宝 H5 支付地址
///
/// 接收订单号并转交应用服务生成 H5 跳转链接
#[utoipa::path(
    get,
    path = "/Pay/CreateAliPayH5",
    params(PaymentOrderQuery),
    responses(
        (status = 200, description = "创建成功", body = String),
        (status = 400, description = "订单或配置无效"),
        (status = 204, description = "订单、状态或创建配置不满足条件"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn create_alipay_h5(
    State(app_state): State<Arc<AppState>>,
    request_context: RequestContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay()?;

    // 支付宝异步通知固定回调到当前宿主公开地址
    let notify_url = format!("{}/Pay/AliPayNotify", request_context.base_url());

    match service
        .create_alipay_h5(&query.order_no, &notify_url, None, None)
        .await
    {
        Ok(Some(result)) => {
            Ok((StatusCode::OK, webapi_core::text_response::Text(result)).into_response())
        }
        Ok(None) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建支付宝小程序支付交易号
///
/// 接收订单号并返回支付宝小程序调起支付所需的 trade_no
#[utoipa::path(
    get,
    path = "/Pay/CreateAliPayMiniApp",
    params(PaymentOrderQuery),
    responses(
        (status = 200, description = "创建成功，返回 trade_no", body = String),
        (status = 400, description = "订单、绑定关系或配置无效"),
        (status = 204, description = "订单、买家绑定或创建配置不存在"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn create_alipay_mini_app(
    State(app_state): State<Arc<AppState>>,
    request_context: RequestContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay()?;

    // 支付宝异步通知固定回调到当前宿主公开地址
    let notify_url = format!("{}/api/Pay/AliPayNotify", request_context.base_url());

    match service
        .create_alipay_mini_app(&query.order_no, &notify_url)
        .await
    {
        Ok(Some(result)) => {
            Ok((StatusCode::OK, webapi_core::text_response::Text(result)).into_response())
        }
        Ok(None) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 处理支付宝异步通知
///
/// 接收支付宝表单回调并将原始参数透传给应用服务验签处理
#[utoipa::path(
    post,
    path = "/Pay/AliPayNotify",
    request_body(content = Object, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "处理完成，返回支付宝要求的 success 文本", body = String),
        (status = 400, description = "通知参数无效"),
        (status = 204, description = "未产生业务确认结果"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn alipay_notify(
    State(app_state): State<Arc<AppState>>,
    FormFields(parameters): FormFields,
) -> Result<Response, AppError> {
    let service = app_state.services().pay()?;

    match service.process_alipay_notify(parameters).await {
        Ok(Some(message)) => Ok(Text(message).into_response()),
        Ok(None) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建微信支付 JSAPI 预支付参数
///
/// 向微信支付下单并返回前端调起支付所需的签名参数，适用于公众号、小程序等微信内场景
#[utoipa::path(
    get,
    path = "/Pay/CreateWeiXinPayJSAPI",
    params(CreateWxPayJsapiQuery),
    responses(
        (status = 200, description = "创建成功", body = application_model::pay::models::WxPayJsapiDto),
        (status = 400, description = "订单或配置无效"),
        (status = 204, description = "订单或创建配置不存在"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn create_wx_pay_jsapi(
    State(app_state): State<Arc<AppState>>,
    request_context: RequestContext,
    Query(query): Query<CreateWxPayJsapiQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay()?;

    // 通知地址不携带商户号后缀，由 Service 层在末尾追加
    let notify_url = format!("{}/Pay/WeiXinPayNotify/", request_context.base_url());

    match service
        .create_wx_pay_jsapi(&query.order_no, &query.open_id, &notify_url)
        .await
    {
        Ok(Some(result)) => Ok((StatusCode::OK, Json(result)).into_response()),
        Ok(None) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建微信支付 App 预支付参数
///
/// 向微信支付下单并返回原生 App 调起支付所需的签名参数
#[utoipa::path(
    get,
    path = "/Pay/CreateWeiXinPayApp",
    params(PaymentOrderQuery),
    responses(
        (status = 200, description = "创建成功", body = application_model::pay::models::WxPayAppDto),
        (status = 400, description = "订单或配置无效"),
        (status = 204, description = "订单或创建配置不存在"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn create_wx_pay_app(
    State(app_state): State<Arc<AppState>>,
    request_context: RequestContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay()?;

    // 通知地址不携带商户号后缀，由 Service 层在末尾追加
    let notify_url = format!("{}/Pay/WeiXinPayNotify/", request_context.base_url());

    match service
        .create_wx_pay_app(&query.order_no, &notify_url)
        .await
    {
        Ok(Some(result)) => Ok((StatusCode::OK, Json(result)).into_response()),
        Ok(None) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建微信支付 H5 支付链接
///
/// 向微信支付下单并返回移动端浏览器跳转支付链接，客户端 IP 由服务端从请求头提取
#[utoipa::path(
    get,
    path = "/Pay/CreateWeiXinPayH5",
    params(PaymentOrderQuery),
    responses(
        (status = 200, description = "创建成功，返回 H5 支付链接", body = String),
        (status = 400, description = "订单或配置无效"),
        (status = 204, description = "订单或创建配置不存在"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn create_wx_pay_h5(
    State(app_state): State<Arc<AppState>>,
    request_context: RequestContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay()?;
    let client_ip = request_context.client_ip()?;

    // 通知地址不携带商户号后缀，由 Service 层在末尾追加
    let notify_url = format!("{}/Pay/WeiXinPayNotify/", request_context.base_url());

    match service
        .create_wx_pay_h5(&query.order_no, &notify_url, client_ip)
        .await
    {
        Ok(Some(result)) => {
            Ok((StatusCode::OK, webapi_core::text_response::Text(result)).into_response())
        }
        Ok(None) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建微信支付 PC 扫码支付链接
///
/// 向微信支付下单并返回二维码内容链接，前端将其渲染为二维码图片供用户扫码
#[utoipa::path(
    get,
    path = "/Pay/CreateWeiXinPayPC",
    params(PaymentOrderQuery),
    responses(
        (status = 200, description = "创建成功，返回二维码链接", body = String),
        (status = 400, description = "订单或配置无效"),
        (status = 204, description = "订单或创建配置不存在"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn create_wx_pay_pc(
    State(app_state): State<Arc<AppState>>,
    request_context: RequestContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay()?;

    // 通知地址不携带商户号后缀，由 Service 层在末尾追加
    let notify_url = format!("{}/Pay/WeiXinPayNotify/", request_context.base_url());

    match service.create_wx_pay_pc(&query.order_no, &notify_url).await {
        Ok(Some(result)) => {
            Ok((StatusCode::OK, webapi_core::text_response::Text(result)).into_response())
        }
        Ok(None) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 处理微信支付异步回调
///
/// 路径中的 `{mch_id}` 由下单时附加到通知地址末尾，微信回调时原样携带，用于区分多商户
/// 与 NetEngine 对齐：处理失败时返回 HTTP 501，成功时返回 HTTP 200
#[utoipa::path(
    post,
    path = "/Pay/WeiXinPayNotify/{mch_id}",
    params(
        ("mch_id" = String, Path, description = "商户号")
    ),
    request_body(content = WxPayNotifyDto, content_type = "application/json"),
    responses(
        (status = 200, description = "处理完成"),
        (status = 501, description = "处理失败，要求微信稍后重试"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn wx_pay_notify(
    State(app_state): State<Arc<AppState>>,
    axum::extract::Path(mch_id): axum::extract::Path<String>,
    headers: HeaderMap,
    JsonWithBody {
        value: notify_dto,
        body,
    }: JsonWithBody<WxPayNotifyDto>,
) -> Result<Response, AppError> {
    // 与 StringValues.ToString 一致，重复请求头按出现顺序合并。
    let headers_map: HashMap<String, String> = headers
        .keys()
        .map(|name| {
            let value = headers
                .get_all(name)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .collect::<Vec<_>>()
                .join(",");
            (name.to_string(), value)
        })
        .collect();

    let service = app_state.services().pay()?;

    // 服务层统一捕获处理异常，失败描述不包含内部错误或通知正文。
    let result = service
        .process_wx_pay_notify(&mch_id, notify_dto, &headers_map, &body)
        .await?;

    let status_code = if result.code == "SUCCESS" {
        StatusCode::OK
    } else {
        StatusCode::NOT_IMPLEMENTED
    };

    Ok((status_code, Json(result)).into_response())
}
