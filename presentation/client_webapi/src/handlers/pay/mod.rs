use std::{collections::HashMap, sync::Arc};

use application_model::pay::models::{CreateWxPayJsapiQuery, PaymentOrderQuery, WxPayNotifyDto};
use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use common::app_error::AppError as ServiceError;
use webapi_core::extract::{Json, Query};
use webapi_core::{auth::UserContext, error::AppError};

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
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "支付"
)]
pub async fn create_alipay_pc(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay();

    // 支付宝异步通知固定回调到当前宿主公开地址
    let notify_url = format!(
        "{}/Pay/AliPayNotify",
        app_state
            .settings
            .file
            .public_base_url
            .trim_end_matches('/')
    );

    match service
        .create_alipay_pc(&query.order_no, &notify_url, None, &user_ctx.current_user())
        .await
    {
        Ok(result) => Ok((StatusCode::OK, result).into_response()),
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
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "支付"
)]
pub async fn create_alipay_h5(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay();

    // 支付宝异步通知固定回调到当前宿主公开地址
    let notify_url = format!(
        "{}/Pay/AliPayNotify",
        app_state
            .settings
            .file
            .public_base_url
            .trim_end_matches('/')
    );

    match service
        .create_alipay_h5(
            &query.order_no,
            &notify_url,
            None,
            None,
            &user_ctx.current_user(),
        )
        .await
    {
        Ok(result) => Ok((StatusCode::OK, result).into_response()),
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
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "支付"
)]
pub async fn create_alipay_mini_app(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay();

    // 支付宝异步通知固定回调到当前宿主公开地址
    let notify_url = format!(
        "{}/Pay/AliPayNotify",
        app_state
            .settings
            .file
            .public_base_url
            .trim_end_matches('/')
    );

    match service
        .create_alipay_mini_app(&query.order_no, &notify_url, &user_ctx.current_user())
        .await
    {
        Ok(result) => Ok((StatusCode::OK, result).into_response()),
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
        (status = 401, description = "签名校验失败"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "支付"
)]
pub async fn alipay_notify(
    State(app_state): State<Arc<AppState>>,
    Form(parameters): Form<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay();

    match service.process_alipay_notify(parameters).await {
        Ok(message) => Ok((
            StatusCode::OK,
            [(CONTENT_TYPE, "text/plain; charset=utf-8")],
            message,
        )
            .into_response()),
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
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "支付"
)]
pub async fn create_wx_pay_jsapi(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<CreateWxPayJsapiQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay();

    // 通知地址不携带商户号后缀，由 Service 层在末尾追加
    let notify_url = format!(
        "{}/Pay/WeiXinPayNotify/",
        app_state
            .settings
            .file
            .public_base_url
            .trim_end_matches('/')
    );

    match service
        .create_wx_pay_jsapi(
            &query.order_no,
            &query.open_id,
            &notify_url,
            &user_ctx.current_user(),
        )
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
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
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "支付"
)]
pub async fn create_wx_pay_app(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay();

    // 通知地址不携带商户号后缀，由 Service 层在末尾追加
    let notify_url = format!(
        "{}/Pay/WeiXinPayNotify/",
        app_state
            .settings
            .file
            .public_base_url
            .trim_end_matches('/')
    );

    match service
        .create_wx_pay_app(&query.order_no, &notify_url, &user_ctx.current_user())
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
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
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "支付"
)]
pub async fn create_wx_pay_h5(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    headers: HeaderMap,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay();
    let client_ip = extract_client_ip(&headers);

    // 通知地址不携带商户号后缀，由 Service 层在末尾追加
    let notify_url = format!(
        "{}/Pay/WeiXinPayNotify/",
        app_state
            .settings
            .file
            .public_base_url
            .trim_end_matches('/')
    );

    match service
        .create_wx_pay_h5(
            &query.order_no,
            &notify_url,
            &client_ip,
            &user_ctx.current_user(),
        )
        .await
    {
        Ok(result) => Ok((StatusCode::OK, result).into_response()),
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
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "支付"
)]
pub async fn create_wx_pay_pc(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<PaymentOrderQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().pay();

    // 通知地址不携带商户号后缀，由 Service 层在末尾追加
    let notify_url = format!(
        "{}/Pay/WeiXinPayNotify/",
        app_state
            .settings
            .file
            .public_base_url
            .trim_end_matches('/')
    );

    match service
        .create_wx_pay_pc(&query.order_no, &notify_url, &user_ctx.current_user())
        .await
    {
        Ok(result) => Ok((StatusCode::OK, result).into_response()),
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
    body: String,
) -> Result<Response, AppError> {
    // 先反序列化通知体，再将原始 body 字符串传给 Service 层做签名验证
    let notify_dto: WxPayNotifyDto = serde_json::from_str(&body).map_err(|error| {
        AppError::from(ServiceError::bad_request(format!(
            "微信支付回调 JSON 格式无效: {error}"
        )))
    })?;

    // 将 HeaderMap 转换为普通 HashMap，便于 Service 层不依赖 axum 类型
    let headers_map: HashMap<String, String> = headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.to_string(), v.to_string()))
        })
        .collect();

    let service = app_state.services().pay();

    // 服务层异常时降级为 FAIL 响应，与 NetEngine 一样交由 501 状态触发微信重试
    let result = service
        .process_wx_pay_notify(&mch_id, notify_dto, &headers_map, &body)
        .await
        .unwrap_or_else(|error| {
            tracing::error!(target: "pay.wx_pay.notify", mch_id = %mch_id, error = %error.message(), "处理微信支付回调异常");
            application_model::pay::models::WxPayNotifyRetDto {
                code: "FAIL".to_string(),
                message: Some(error.message().to_string()),
            }
        });

    let status_code = if result.code == "SUCCESS" {
        StatusCode::OK
    } else {
        StatusCode::NOT_IMPLEMENTED
    };

    Ok((status_code, Json(result)).into_response())
}

/// 从请求头中提取客户端 IP
fn extract_client_ip(headers: &HeaderMap) -> String {
    if let Some(forwarded_for) = headers.get("x-forwarded-for")
        && let Ok(value) = forwarded_for.to_str()
    {
        let ip = value.split(',').next().unwrap_or("").trim();
        if !ip.is_empty() {
            return ip.to_string();
        }
    }

    if let Some(real_ip) = headers.get("x-real-ip")
        && let Ok(value) = real_ip.to_str()
    {
        let ip = value.trim();
        if !ip.is_empty() {
            return ip.to_string();
        }
    }

    "unknown".to_string()
}
