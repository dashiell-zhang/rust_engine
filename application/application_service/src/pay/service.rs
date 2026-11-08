use repository::extensions::SoftDeleteEntityExt;
use repository_core::versioned::VersionedActiveModel;
use std::{collections::HashMap, future::Future, str::FromStr, time::Duration};

use base64::Engine;
use chrono::{FixedOffset, NaiveDateTime, Utc};
use distributed_cache::{CacheEntryOptions, DistributedCacheExt};
use repository::{app_setting, order, user_bind_external};
use sea_orm::{ActiveValue::Set, ColumnTrait, IntoActiveModel, QueryFilter, prelude::Decimal};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::form_urlencoded;

use application_interface::{context::PayContext, error::AppError};
use application_model::pay::models::{
    WxPayAppDto, WxPayJsapiDto, WxPayNotifyDto, WxPayNotifyRefundDto, WxPayNotifyRetDto,
    WxPayNotifyTransactionDto, WxPayRefundResultDto,
};

/// 支付能力应用服务实现
///
/// 负责统一封装支付宝与微信支付的下单、签名校验与回调落库逻辑
/// 支付宝网页支付模块名
const ALIPAY_WEB_MODULE: &str = "AliPayWeb";

/// 支付宝小程序支付模块名
const ALIPAY_MINI_APP_MODULE: &str = "AliPayMiniApp";

/// 支付宝网关地址
const ALIPAY_GATEWAY_URL: &str = "https://openapi.alipay.com/gateway.do";

/// 对齐 NetEngine 当前使用的 SDK 协议元数据，不表示运行时依赖 .NET SDK。
const ALIPAY_SDK_PROTOCOL: &str = "alipay-sdk-net-4.9.1424.ALL";

/// 微信平台证书缓存秒数
const WX_PAY_CERT_CACHE_TTL_SECONDS: u64 = 3600;

/// 微信预支付结果缓存秒数
const WX_PAY_CREATE_CACHE_TTL_SECONDS: u64 = 6480;

/// 支付模块应用服务
pub struct PayService<'a> {
    /// 支付模块上下文
    context: PayContext<'a>,
}

impl<'a> PayService<'a> {
    /// 创建支付模块应用服务
    pub fn new(context: PayContext<'a>) -> Self {
        Self { context }
    }

    /// 证书缓存键
    fn wx_pay_cert_cache_key(mch_id: &str) -> String {
        format!("{mch_id}GetWeiXinPayCertificates")
    }

    /// 证书互斥锁键
    fn wx_pay_cert_lock_key(mch_id: &str) -> String {
        format!("{mch_id}GetWeiXinPayCertificateslock")
    }

    /// 读取 JSON 缓存
    async fn get_cached_json<T>(&self, key: &str) -> Option<T>
    where
        T: DeserializeOwned,
    {
        // 对象缓存扩展将读取和反序列化失败视为未命中，不删除原缓存。
        let value = self.context.cache.get_string(key).await.ok().flatten()?;
        common::json_helper::json_to_object(&value).ok()
    }

    /// 尝试写入 JSON 缓存
    async fn try_set_cached_json<T>(&self, key: &str, value: &T, ttl: Duration)
    where
        T: Serialize,
    {
        if let Ok(serialized) = serde_json::to_string(value) {
            let _ = self
                .context
                .cache
                .set_string_with_options(key, &serialized, CacheEntryOptions::relative(ttl))
                .await;
        }
    }

    /// JSAPI 和 App 预支付使用对象缓存；未命中直接请求，不增加锁。
    async fn get_or_create_wx_cached_result<T, F, Fut>(
        &self,
        cache_key: &str,
        producer: F,
    ) -> Result<Option<T>, AppError>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Option<T>, AppError>>,
    {
        if let Some(cached_value) = self.get_cached_json(cache_key).await {
            return Ok(Some(cached_value));
        }

        let result = producer().await?;
        if let Some(value) = &result {
            self.try_set_cached_json(
                cache_key,
                value,
                Duration::from_secs(WX_PAY_CREATE_CACHE_TTL_SECONDS),
            )
            .await;
        }

        Ok(result)
    }

    /// H5 和 PC 链接缓存原始字符串；读取故障直接传播，空字符串视为未命中。
    async fn get_or_create_wx_cached_link<F, Fut>(
        &self,
        cache_key: &str,
        ttl: Duration,
        producer: F,
    ) -> Result<Option<String>, AppError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Option<String>, AppError>>,
    {
        let cached = self.context.cache.get_string(cache_key).await?;
        if let Some(value) = &cached
            && !value.is_empty()
        {
            return Ok(cached);
        }

        let result = producer().await?;
        if let Some(value) = &result {
            let _ = self
                .context
                .cache
                .set_string_with_options(cache_key, value, CacheEntryOptions::relative(ttl))
                .await;
        }
        // 无可创建结果时保留最初读出的空字符串，与源局部变量返回一致。
        Ok(result.or(cached))
    }

    /// 按原始订单号读取有效记录，各创建用例自行声明状态条件。
    async fn load_payment_order(&self, order_no: &str) -> Result<Option<order::Model>, AppError> {
        order::Entity::find_active()
            .filter(order::Column::OrderNo.eq(order_no))
            .one(self.context.db)
            .await
            .map_err(to_app_error)
    }

    /// 创建请求先读取模块内各键首值；JSAPI/App 还需要前端调起签名密钥。
    async fn load_wx_create_settings(
        &self,
        needs_key: bool,
    ) -> Result<Option<WxPayCreateSettings>, AppError> {
        let values = app_setting::Entity::find_active()
            .filter(app_setting::Column::Module.eq(WX_PAY_MODULE))
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;
        let first = |key: &str| {
            values
                .iter()
                .find(|item| item.key == key)
                .map(|item| item.value.clone())
        };
        let (Some(app_id), Some(mch_id)) = (first("AppId"), first("MchId")) else {
            return Ok(None);
        };
        let signing_key = first("MchApiCertKey");
        if needs_key && signing_key.is_none() {
            return Ok(None);
        }
        Ok(Some(WxPayCreateSettings {
            app_id,
            mch_id,
            signing_key,
        }))
    }

    /// HTTP 请求按商户号选中配置组；可空组和未匹配商户均按源查询匹配 NULL。
    async fn load_wx_request_settings(
        &self,
        mch_id: &str,
        app_id: &str,
    ) -> Result<WxPaySettings, AppError> {
        let merchant = app_setting::Entity::find_active()
            .filter(app_setting::Column::Module.eq(WX_PAY_MODULE))
            .filter(app_setting::Column::Key.eq("MchId"))
            .filter(app_setting::Column::Value.eq(mch_id))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;
        let group_id = merchant.map(|item| item.group_id).unwrap_or_default();
        let group = app_setting::Entity::find_active()
            .filter(app_setting::Column::Module.eq(WX_PAY_MODULE))
            .filter(match group_id {
                Some(id) => app_setting::Column::GroupId.eq(id),
                None => app_setting::Column::GroupId.is_null(),
            })
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;
        let required = |key: &str| {
            group
                .iter()
                .find(|item| item.key == key)
                .map(|item| item.value.clone())
                .ok_or_else(|| AppError::internal(format!("微信支付商户配置缺少 {key}")))
        };
        Ok(WxPaySettings {
            app_id: app_id.to_string(),
            mch_id: mch_id.to_string(),
            mch_api_cert_id: required("MchApiCertId")?,
            mch_api_cert_key: required("MchApiCertKey")?,
            mch_api_v3_key: required("MchApiV3Key")?,
        })
    }

    /// 创建支付宝 PC 支付地址
    ///
    /// 读取订单与支付宝配置后，生成可直接跳转的网页支付链接
    pub async fn create_alipay_pc(
        &self,
        order_no: &str,
        notify_url: &str,
        return_url: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let Some(settings) = self.load_alipay_create_settings(ALIPAY_WEB_MODULE).await? else {
            return Ok(None);
        };
        let Some(order_model) = self
            .load_payment_order(order_no)
            .await?
            .filter(|order| order.state == "待支付")
        else {
            return Ok(None);
        };
        let biz_content = json!({
            "body": "商品描述",
            "subject": build_order_title(&order_model),
            "total_amount": order_model.price.to_string(),
            "out_trade_no": order_model.order_no,
            "product_code": "FAST_INSTANT_TRADE_PAY",
            "qr_pay_mode": "4",
        });

        self.build_alipay_gateway_url(
            &settings,
            "alipay.trade.page.pay",
            biz_content,
            notify_url,
            return_url,
        )
        .map(Some)
    }

    /// 创建支付宝 H5 支付地址
    ///
    /// 读取订单与支付宝配置后，生成适用于移动端浏览器的支付链接
    pub async fn create_alipay_h5(
        &self,
        order_no: &str,
        notify_url: &str,
        return_url: Option<&str>,
        quit_url: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let Some(settings) = self.load_alipay_create_settings(ALIPAY_WEB_MODULE).await? else {
            return Ok(None);
        };
        let Some(order_model) = self
            .load_payment_order(order_no)
            .await?
            .filter(|order| order.state == "待支付")
        else {
            return Ok(None);
        };
        let biz_content = json!({
            "body": "商品描述",
            "subject": build_order_title(&order_model),
            "total_amount": order_model.price.to_string(),
            "out_trade_no": order_model.order_no,
            "product_code": "QUICK_WAP_WAY",
            "quit_url": quit_url.unwrap_or_default(),
        });

        self.build_alipay_gateway_url(
            &settings,
            "alipay.trade.wap.pay",
            biz_content,
            notify_url,
            return_url,
        )
        .map(Some)
    }

    /// 创建支付宝小程序支付交易号
    ///
    /// 读取订单 支付宝小程序配置与用户绑定 OpenId 后 创建服务端支付单并返回 trade_no
    pub async fn create_alipay_mini_app(
        &self,
        order_no: &str,
        notify_url: &str,
    ) -> Result<Option<String>, AppError> {
        let Some(settings) = self
            .load_alipay_create_settings(ALIPAY_MINI_APP_MODULE)
            .await?
        else {
            return Ok(None);
        };
        let Some(order_model) = self.load_payment_order(order_no).await? else {
            return Ok(None);
        };
        // 买家来自订单创建人的绑定，不从当前 HTTP 用户推断。
        let Some(buyer_id) = self
            .load_alipay_mini_app_buyer_id(order_model.create_user_id, &settings.app_id)
            .await?
        else {
            return Ok(None);
        };

        let biz_content = json!({
            "subject": build_order_title(&order_model),
            "total_amount": order_model.price.to_string(),
            "out_trade_no": order_model.order_no,
            "buyer_id": buyer_id,
        });

        let response = self
            .execute_alipay_api(
                &settings,
                "alipay.trade.create",
                biz_content,
                Some(notify_url),
                "utf-8",
            )
            .await?;

        extract_alipay_trade_no(&response).map(Some)
    }

    /// 处理支付宝异步通知
    ///
    /// 校验支付宝签名后保存订单，仅业务逻辑订单返回确认文本。
    pub async fn process_alipay_notify(
        &self,
        parameters: HashMap<String, String>,
    ) -> Result<Option<String>, AppError> {
        if parameters.is_empty() {
            return Ok(None);
        }

        let public_key = self
            .load_alipay_notify_public_key(parameters.get("app_id").map(String::as_str))
            .await?;
        if !verify_alipay_notify_signature(&parameters, public_key.as_deref().unwrap_or_default())?
        {
            return Ok(None);
        }

        let Some(order_no) = parameters.get("out_trade_no") else {
            return Ok(None);
        };
        let Some(order_model) = self.load_payment_order(order_no).await? else {
            return Ok(None);
        };

        let total_amount = parameters
            .get("total_amount")
            .map(String::as_str)
            .ok_or_else(|| AppError::internal("缺少 total_amount"))?;
        let pay_price = Decimal::from_str(total_amount.trim())
            .map_err(|_| AppError::internal("支付宝通知金额格式无效"))?;
        let trade_no = parameters.get("trade_no").cloned();
        let paid_at = parse_alipay_paid_time(parameters.get("gmt_payment").map(String::as_str))?;
        let changed = order_model.pay_price != Some(pay_price)
            || order_model.serial_no != trade_no
            || !order_model.pay_state
            || order_model.pay_time != Some(paid_at.into())
            || order_model.pay_type != "支付宝"
            || order_model.state != "已支付";
        let acknowledges = order_model.r#type == "业务逻辑";
        // EF 跟踪值没有变化时不会写入，也不刷新 UpdateTime。
        if changed {
            let mut active = order_model.into_active_model();
            active.pay_price = Set(Some(pay_price));
            active.serial_no = Set(trade_no);
            active.pay_state = Set(true);
            active.pay_time = Set(Some(paid_at.into()));
            active.pay_type = Set("支付宝".to_string());
            active.state = Set("已支付".to_string());
            active
                .update_versioned(self.context.db)
                .await
                .map_err(|error| AppError::internal(format!("更新订单支付状态失败: {error}")))?;
        }

        Ok(acknowledges.then(|| "success".to_string()))
    }

    /// 回调先定位 AliPay 前缀模块的应用，再从其配置组读取公钥。
    async fn load_alipay_notify_public_key(
        &self,
        app_id: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        let group_id = if let Some(app_id) = app_id {
            app_setting::Entity::find_active()
                .filter(app_setting::Column::Module.starts_with("AliPay"))
                .filter(app_setting::Column::Key.eq("AppId"))
                .filter(app_setting::Column::Value.eq(app_id))
                .one(self.context.db)
                .await
                .map_err(to_app_error)?
                .and_then(|item| item.group_id)
        } else {
            None
        };
        // 源查询没有匹配应用时同样选择 NULL 组，组内不再限制 Module。
        let query = app_setting::Entity::find_active();
        let values = match group_id {
            Some(id) => query.filter(app_setting::Column::GroupId.eq(id)),
            None => query.filter(app_setting::Column::GroupId.is_null()),
        }
        .all(self.context.db)
        .await
        .map_err(to_app_error)?;
        Ok(values
            .into_iter()
            .find(|item| item.key == "AliPayPublicKey")
            .map(|item| item.value))
    }

    /// 创建用例使用模块内各键首值，缺失时返回空结果，不按 AppId 限制配置组。
    async fn load_alipay_create_settings(
        &self,
        module: &str,
    ) -> Result<Option<AliPayWebSettings>, AppError> {
        let values = app_setting::Entity::find_active()
            .filter(app_setting::Column::Module.eq(module))
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;
        let first = |key: &str| {
            values
                .iter()
                .find(|item| item.key == key)
                .map(|item| item.value.clone())
        };
        let (Some(app_id), Some(app_private_key), Some(alipay_public_key)) = (
            first("AppId"),
            first("AppPrivateKey"),
            first("AliPayPublicKey"),
        ) else {
            return Ok(None);
        };
        Ok(Some(AliPayWebSettings {
            app_id,
            app_private_key,
            alipay_public_key,
            gateway_url: ALIPAY_GATEWAY_URL.into(),
        }))
    }

    /// 退款模板按模块首值配置 SDK；空值交由签名与提供商协议处理。
    async fn load_alipay_refund_settings(&self) -> Result<AliPayWebSettings, AppError> {
        let values = app_setting::Entity::find_active()
            .filter(app_setting::Column::Module.eq(ALIPAY_WEB_MODULE))
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;
        let first = |key: &str| {
            values
                .iter()
                .find(|item| item.key == key)
                .map(|item| item.value.clone())
                .unwrap_or_default()
        };
        Ok(AliPayWebSettings {
            app_id: first("AppId"),
            app_private_key: first("AppPrivateKey"),
            alipay_public_key: first("AliPayPublicKey"),
            gateway_url: ALIPAY_GATEWAY_URL.into(),
        })
    }

    /// 执行支付宝开放接口调用
    ///
    /// 以表单方式调用支付宝网关并返回原始 JSON 包装体
    async fn execute_alipay_api(
        &self,
        settings: &AliPayWebSettings,
        method: &str,
        biz_content: Value,
        notify_url: Option<&str>,
        charset: &str,
    ) -> Result<Value, AppError> {
        let mut parameters = vec![
            ("alipay_sdk".to_string(), ALIPAY_SDK_PROTOCOL.to_string()),
            ("app_id".to_string(), settings.app_id.clone()),
            ("biz_content".to_string(), biz_content.to_string()),
            ("charset".to_string(), charset.to_string()),
            ("format".to_string(), "json".to_string()),
            ("method".to_string(), method.to_string()),
            ("sign_type".to_string(), "RSA2".to_string()),
            (
                "timestamp".to_string(),
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ),
            ("version".to_string(), "1.0".to_string()),
        ];

        if let Some(notify_url) = notify_url.filter(|value| !value.is_empty()) {
            parameters.push(("notify_url".to_string(), notify_url.to_string()));
        }

        // 与网页支付相同，先按字典序拼接待签名内容
        parameters.retain(|(_, value)| !value.is_empty());
        parameters.sort_by(|left, right| left.0.cmp(&right.0));
        let content = build_sign_content(&parameters);
        let sign = sign_alipay_content(&settings.app_private_key, &content)?;
        parameters.push(("sign".to_string(), sign));

        let body = parameters
            .iter()
            .fold(
                form_urlencoded::Serializer::new(String::new()),
                |mut serializer, (key, value)| {
                    serializer.append_pair(key, value);
                    serializer
                },
            )
            .finish();

        let response = self
            .context
            .http_client_factory
            .client()
            .post(&settings.gateway_url)
            .query(&[("charset", charset)])
            .header(
                reqwest::header::CONTENT_TYPE,
                format!("application/x-www-form-urlencoded;charset={charset}"),
            )
            .body(body)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("调用支付宝接口失败: {error}")))?;

        let response_text = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("读取支付宝接口响应失败: {error}")))?;

        super::alipay_response::parse_response(&response_text, method, &settings.alipay_public_key)
    }

    /// 加载支付宝小程序买家标识
    ///
    /// 从用户外部绑定表中读取订单创建人在支付宝小程序下的 OpenId。
    async fn load_alipay_mini_app_buyer_id(
        &self,
        user_id: Option<i64>,
        app_id: &str,
    ) -> Result<Option<String>, AppError> {
        let Some(user_id) = user_id else {
            return Ok(None);
        };
        let buyer_id = user_bind_external::Entity::find_active()
            .filter(user_bind_external::Column::UserId.eq(user_id))
            .filter(user_bind_external::Column::AppName.eq(ALIPAY_MINI_APP_MODULE))
            .filter(user_bind_external::Column::AppId.eq(app_id))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .map(|item| item.open_id);

        Ok(buyer_id)
    }

    /// 组装支付宝网关地址
    ///
    /// 按支付宝要求拼接公共参数并完成 RSA2 签名
    fn build_alipay_gateway_url(
        &self,
        settings: &AliPayWebSettings,
        method: &str,
        biz_content: Value,
        notify_url: &str,
        return_url: Option<&str>,
    ) -> Result<String, AppError> {
        let mut parameters = vec![
            ("alipay_sdk".to_string(), ALIPAY_SDK_PROTOCOL.to_string()),
            ("app_id".to_string(), settings.app_id.clone()),
            ("biz_content".to_string(), biz_content.to_string()),
            ("charset".to_string(), "UTF-8".to_string()),
            ("format".to_string(), "json".to_string()),
            ("method".to_string(), method.to_string()),
            ("notify_url".to_string(), notify_url.to_string()),
            ("sign_type".to_string(), "RSA2".to_string()),
            (
                "timestamp".to_string(),
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ),
            ("version".to_string(), "1.0".to_string()),
        ];

        if let Some(value) = return_url.filter(|value| !value.is_empty()) {
            parameters.push(("return_url".to_string(), value.to_string()));
        }

        parameters.retain(|(_, value)| !value.is_empty());
        parameters.sort_by(|left, right| left.0.cmp(&right.0));

        let content = build_sign_content(&parameters);
        let sign = sign_alipay_content(&settings.app_private_key, &content)?;
        parameters.push(("sign".to_string(), sign));

        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for (key, value) in parameters {
            serializer.append_pair(&key, &value);
        }

        Ok(format!("{}?{}", settings.gateway_url, serializer.finish()))
    }
}

/// 支付宝网页支付配置
struct AliPayWebSettings {
    /// 应用标识
    app_id: String,

    /// 应用私钥
    app_private_key: String,

    /// 支付宝公钥
    alipay_public_key: String,

    /// 网关地址
    gateway_url: String,
}

/// 构建订单标题
fn build_order_title(order_model: &order::Model) -> String {
    format!("{}交易", order_model.create_time.format("%Y%m%d%H%M"))
}

/// 构建支付宝签名明文
fn build_sign_content(parameters: &[(String, String)]) -> String {
    parameters
        .iter()
        .filter(|(key, value)| key != "sign" && !value.is_empty())
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// 对支付宝参数进行签名
fn sign_alipay_content(private_key_pem: &str, content: &str) -> Result<String, AppError> {
    use common::crypto_helper::{
        RsaHashAlgorithm, RsaSignaturePadding, StringEncoding, rsa_sign_data,
    };
    rsa_sign_data(
        private_key_pem,
        content,
        StringEncoding::Base64,
        RsaHashAlgorithm::Sha256,
        RsaSignaturePadding::Pkcs1,
    )
    .map_err(|error| AppError::internal(format!("支付宝 RSA2 签名失败: {error}")))
}

/// 校验支付宝异步通知签名
fn verify_alipay_notify_signature(
    parameters: &HashMap<String, String>,
    public_key_pem: &str,
) -> Result<bool, AppError> {
    use common::crypto_helper::{
        RsaHashAlgorithm, RsaSignaturePadding, StringEncoding, rsa_verify_data,
    };
    let signature_text = parameters
        .get("sign")
        .map(String::as_str)
        .ok_or_else(|| AppError::internal("支付宝通知缺少签名"))?;

    let mut pairs = parameters
        .iter()
        .filter(|(key, value)| *key != "sign" && *key != "sign_type" && !value.is_empty())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();

    pairs.sort_by(|left, right| left.0.cmp(&right.0));
    let content = build_sign_content(&pairs);

    rsa_verify_data(
        public_key_pem,
        &content,
        signature_text,
        StringEncoding::Base64,
        RsaHashAlgorithm::Sha256,
        RsaSignaturePadding::Pkcs1,
    )
    .map_err(|_| AppError::internal("支付宝通知签名或公钥格式无效"))
}

/// 解析支付宝支付完成时间
fn parse_alipay_paid_time(value: Option<&str>) -> Result<chrono::DateTime<Utc>, AppError> {
    let minimum = chrono::NaiveDate::from_ymd_opt(1, 1, 1)
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .expect("公历最小日期有效");
    // Convert.ToDateTime(null) 为 MinValue；未带偏移的通知使用进程本地时区。
    let Some(value) = value else {
        return minimum
            .and_local_timezone(chrono::Local)
            .latest()
            .map(|date| date.to_utc().max(minimum.and_utc()))
            .ok_or_else(|| AppError::internal("支付时间无法转换为本地时刻"));
    };
    if let Ok(date) = chrono::DateTime::parse_from_rfc3339(value.trim()) {
        return Ok(date.to_utc());
    }
    let naive = NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M:%S")
        .map_err(|_| AppError::internal("支付宝通知支付时间格式无效"))?;
    naive
        .and_local_timezone(chrono::Local)
        .latest()
        .map(|item| item.to_utc().max(minimum.and_utc()))
        .ok_or_else(|| AppError::internal("支付时间无法转换为本地时刻"))
}

/// 提取支付宝小程序下单返回的交易号
///
/// 优先读取 trade_no，缺失时回落到支付宝返回的错误信息
fn extract_alipay_trade_no(response: &Value) -> Result<String, AppError> {
    let payload = extract_alipay_success_payload(
        response,
        "alipay_trade_create_response",
        "支付宝交易订单创建失败",
    )?;

    if let Some(trade_no) = payload
        .get("trade_no")
        .and_then(Value::as_str)
        .filter(|item| !item.trim().is_empty())
    {
        return Ok(trade_no.to_string());
    }

    let sub_msg = payload
        .get("sub_msg")
        .and_then(Value::as_str)
        .or_else(|| payload.get("msg").and_then(Value::as_str))
        .unwrap_or("支付宝交易订单创建失败");

    Err(AppError::bad_request(sub_msg))
}

/// 提取并校验支付宝响应负载
fn extract_alipay_success_payload<'a>(
    response: &'a Value,
    payload_key: &str,
    default_message: &str,
) -> Result<&'a Value, AppError> {
    let payload = response
        .get(payload_key)
        .ok_or_else(|| AppError::internal(format!("支付宝接口响应缺少 {payload_key}")))?;

    if payload.get("code").and_then(Value::as_str) == Some("10000") {
        return Ok(payload);
    }

    let message = payload
        .get("sub_msg")
        .and_then(Value::as_str)
        .or_else(|| payload.get("msg").and_then(Value::as_str))
        .unwrap_or(default_message);

    Err(AppError::bad_request(message))
}

/// 转换数据库异常
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}

// ===== 微信支付 =====

/// 微信支付模块名
const WX_PAY_MODULE: &str = "WeiXinPay";

/// 微信支付 API 基地址
const WX_PAY_BASE_URL: &str = "https://api.mch.weixin.qq.com";

/// 微信支付配置
struct WxPayCreateSettings {
    app_id: String,
    mch_id: String,
    signing_key: Option<String>,
}

/// 微信支付传输与回调配置
struct WxPaySettings {
    /// 应用标识
    app_id: String,

    /// 商户号
    mch_id: String,

    /// API 证书序列号
    mch_api_cert_id: String,

    /// API 证书私钥（PKCS8 PEM）
    mch_api_cert_key: String,

    /// API V3 密钥
    mch_api_v3_key: String,
}

/// 微信支付平台证书条目
#[derive(Debug, Deserialize, Serialize)]
struct WxPayCertEntry {
    #[serde(default = "default_certificate_time", with = "certificate_time")]
    effective_time: chrono::DateTime<FixedOffset>,

    #[serde(default = "default_certificate_time", with = "certificate_time")]
    expire_time: chrono::DateTime<FixedOffset>,

    /// 证书序列号
    serial_no: String,

    /// 加密证书信息
    encrypt_certificate: WxPayEncryptedCert,

    /// 解密结果与证书元数据一起缓存，验签时按序列号选择。
    #[serde(default)]
    certificate: Option<String>,
}

/// 微信支付加密证书信息
#[derive(Debug, Deserialize, Serialize)]
struct WxPayEncryptedCert {
    #[serde(default)]
    algorithm: Option<String>,

    /// 密文
    ciphertext: String,

    /// 附加数据
    associated_data: String,

    /// 随机串
    nonce: String,
}

/// 微信支付平台证书列表
#[derive(Debug, Deserialize, Serialize)]
struct WxPayCertificatesResponse {
    /// 证书列表
    data: Vec<WxPayCertEntry>,
}

fn default_certificate_time() -> chrono::DateTime<FixedOffset> {
    chrono::NaiveDate::from_ymd_opt(1, 1, 1)
        .expect("有效的默认日期")
        .and_hms_opt(0, 0, 0)
        .expect("有效的默认时间")
        .and_utc()
        .fixed_offset()
}

/// 证书接口读取 ISO 时间，缓存沿用公共 DateTimeOffset 的带偏移格式。
mod certificate_time {
    use chrono::{DateTime, FixedOffset};
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        value: &DateTime<FixedOffset>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.format("%Y/%m/%d %H:%M:%S %:z").to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<DateTime<FixedOffset>, D::Error> {
        common::json_helper::date_time::deserialize_offset(deserializer)
    }
}

impl<'a> PayService<'a> {
    /// 创建微信支付 JSAPI 预支付参数
    ///
    /// 向微信支付下单并构造前端调起支付所需的签名参数，适用于微信内 H5 网页支付场景
    pub async fn create_wx_pay_jsapi(
        &self,
        order_no: &str,
        open_id: &str,
        notify_url: &str,
    ) -> Result<Option<WxPayJsapiDto>, AppError> {
        self.get_or_create_wx_cached_result(&format!("wxpayJSAPI{order_no}"), || async move {
            let Some(order_model) = self.load_payment_order(order_no).await? else {
                return Ok(None);
            };
            let Some(creation) = self.load_wx_create_settings(true).await? else {
                return Ok(None);
            };

            let price_fen = decimal_to_fen(order_model.price)?;
            let settings = self
                .load_wx_request_settings(&creation.mch_id, &creation.app_id)
                .await?;
            let full_notify_url = format!("{}{}", notify_url, settings.mch_id);
            let description = format!("{}交易", Utc::now().format("%Y%m%d%H%M"));

            let req_body = json!({
                "mchid": settings.mch_id,
                "out_trade_no": order_model.order_no,
                "appid": settings.app_id,
                "description": description,
                "notify_url": full_notify_url,
                "amount": { "total": price_fen, "currency": "CNY" },
                "payer": { "openid": open_id }
            });

            let url = format!("{}/v3/pay/transactions/jsapi", WX_PAY_BASE_URL);
            let result_json = self.wx_pay_http_post(&settings, &url, &req_body).await?;
            let prepay_id = extract_json_field(&result_json, "prepay_id")?;

            let package = format!("prepay_id={}", prepay_id);
            let time_stamp = Utc::now().timestamp();
            let nonce_str = generate_nonce();
            let message = format!(
                "{}\n{}\n{}\n{}\n",
                settings.app_id, time_stamp, nonce_str, package
            );
            let sign = wx_pay_rsa_sign(
                creation
                    .signing_key
                    .as_deref()
                    .expect("创建前已检查签名配置"),
                &message,
            )?;

            Ok(Some(WxPayJsapiDto {
                app_id: settings.app_id,
                package,
                nonce_str,
                time_stamp: time_stamp.to_string(),
                sign,
            }))
        })
        .await
    }

    /// 创建微信支付 App 预支付参数
    ///
    /// 向微信支付下单并构造原生 App 调起支付所需的签名参数
    pub async fn create_wx_pay_app(
        &self,
        order_no: &str,
        notify_url: &str,
    ) -> Result<Option<WxPayAppDto>, AppError> {
        self.get_or_create_wx_cached_result(&format!("wxpayApp{order_no}"), || async move {
            let Some(order_model) = self.load_payment_order(order_no).await? else {
                return Ok(None);
            };
            let Some(creation) = self.load_wx_create_settings(true).await? else {
                return Ok(None);
            };

            let price_fen = decimal_to_fen(order_model.price)?;
            let settings = self
                .load_wx_request_settings(&creation.mch_id, &creation.app_id)
                .await?;
            let full_notify_url = format!("{}{}", notify_url, settings.mch_id);
            let description = format!("{}交易", Utc::now().format("%Y%m%d%H%M"));

            let req_body = json!({
                "mchid": settings.mch_id,
                "out_trade_no": order_model.order_no,
                "appid": settings.app_id,
                "description": description,
                "notify_url": full_notify_url,
                "amount": { "total": price_fen, "currency": "CNY" }
            });

            let url = format!("{}/v3/pay/transactions/app", WX_PAY_BASE_URL);
            let result_json = self.wx_pay_http_post(&settings, &url, &req_body).await?;
            let prepay_id = extract_json_field(&result_json, "prepay_id")?;

            let time_stamp = Utc::now().timestamp();
            let nonce_str = generate_nonce();
            let message = format!(
                "{}\n{}\n{}\n{}\n",
                settings.app_id, time_stamp, nonce_str, prepay_id
            );
            let sign = wx_pay_rsa_sign(
                creation
                    .signing_key
                    .as_deref()
                    .expect("创建前已检查签名配置"),
                &message,
            )?;

            Ok(Some(WxPayAppDto {
                app_id: settings.app_id,
                partner_id: settings.mch_id,
                prepay_id,
                nonce_str,
                time_stamp: time_stamp.to_string(),
                sign,
            }))
        })
        .await
    }

    /// 创建微信支付 H5 支付链接
    ///
    /// 向微信支付下单并返回 H5 跳转支付链接，适用于移动端浏览器场景，需提供客户端 IP
    pub async fn create_wx_pay_h5(
        &self,
        order_no: &str,
        notify_url: &str,
        client_ip: &str,
    ) -> Result<Option<String>, AppError> {
        self.get_or_create_wx_cached_link(
            &format!("wxpayH5Url{order_no}"),
            Duration::from_secs(270),
            || async move {
                let Some(order_model) = self.load_payment_order(order_no).await? else {
                    return Ok(None);
                };
                let Some(creation) = self.load_wx_create_settings(false).await? else {
                    return Ok(None);
                };

                let price_fen = decimal_to_fen(order_model.price)?;
                let settings = self
                    .load_wx_request_settings(&creation.mch_id, &creation.app_id)
                    .await?;
                let full_notify_url = format!("{}{}", notify_url, settings.mch_id);
                let description = format!("{}交易", Utc::now().format("%Y%m%d%H%M"));

                let req_body = json!({
                    "mchid": settings.mch_id,
                    "out_trade_no": order_model.order_no,
                    "appid": settings.app_id,
                    "description": description,
                    "notify_url": full_notify_url,
                    "amount": { "total": price_fen, "currency": "CNY" },
                    "scene_info": {
                        "payer_client_ip": client_ip,
                        "h5_info": { "type": "Wap" }
                    }
                });

                let url = format!("{}/v3/pay/transactions/h5", WX_PAY_BASE_URL);
                let result_json = self.wx_pay_http_post(&settings, &url, &req_body).await?;
                extract_json_field(&result_json, "h5_url").map(Some)
            },
        )
        .await
    }

    /// 创建微信支付 PC 扫码支付链接
    ///
    /// 向微信支付下单并返回二维码链接，结果缓存十五分钟。
    pub async fn create_wx_pay_pc(
        &self,
        order_no: &str,
        notify_url: &str,
    ) -> Result<Option<String>, AppError> {
        self.get_or_create_wx_cached_link(
            &format!("wxpayPCUrl{order_no}"),
            Duration::from_secs(900),
            || async move {
                let Some(order_model) = self.load_payment_order(order_no).await? else {
                    return Ok(None);
                };
                let Some(creation) = self.load_wx_create_settings(false).await? else {
                    return Ok(None);
                };

                let price_fen = decimal_to_fen(order_model.price)?;
                let settings = self
                    .load_wx_request_settings(&creation.mch_id, &creation.app_id)
                    .await?;
                let full_notify_url = format!("{}{}", notify_url, settings.mch_id);
                let description = format!("{}交易", Utc::now().format("%Y%m%d%H%M"));

                let req_body = json!({
                    "mchid": settings.mch_id,
                    "out_trade_no": order_model.order_no,
                    "appid": settings.app_id,
                    "description": description,
                    "notify_url": full_notify_url,
                    "amount": { "total": price_fen, "currency": "CNY" }
                });

                let url = format!("{}/v3/pay/transactions/native", WX_PAY_BASE_URL);
                let result_json = self.wx_pay_http_post(&settings, &url, &req_body).await?;
                extract_json_field(&result_json, "code_url").map(Some)
            },
        )
        .await
    }

    /// 处理微信支付异步回调
    ///
    /// 依次完成签名验证、密文解密、订单状态更新；HTTP 入口按成功或失败选择状态码。
    pub async fn process_wx_pay_notify(
        &self,
        mch_id: &str,
        notify_dto: WxPayNotifyDto,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<WxPayNotifyRetDto, AppError> {
        let mut success = false;
        let result: Result<(), AppError> = async {
            let certificates = self.fetch_wx_pay_certificates(mch_id).await?;
            verify_wx_pay_response_sign(headers, body, &certificates)
                .map_err(AppError::internal)?;
            if notify_dto.event_type == "TRANSACTION.SUCCESS" {
                let api_key = self.load_wx_notify_key().await?;
                let plaintext = aes_gcm_decrypt_wx(&notify_dto.resource, &api_key)?;
                let transaction: WxPayNotifyTransactionDto =
                    common::json_helper::json_to_object(&plaintext)
                        .map_err(|_| AppError::internal("微信交易通知明文格式无效"))?;
                if transaction.trade_state.as_deref() == Some("SUCCESS") {
                    success = self.apply_wx_pay_transaction(&transaction).await?;
                }
            }
            if notify_dto.resource.original_type == "refund" {
                let api_key = self.load_wx_notify_key().await?;
                let plaintext = aes_gcm_decrypt_wx(&notify_dto.resource, &api_key)?;
                // 源模板只解析退款通知，尚未按退款状态执行业务动作。
                let _refund: WxPayNotifyRefundDto = common::json_helper::json_to_object(&plaintext)
                    .map_err(|_| AppError::internal("微信退款通知明文格式无效"))?;
                success = true;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            tracing::error!(target: "pay.wx_pay.notify", mch_id = %mch_id, error = %error.message(), "处理微信支付回调异常");
        }
        Ok(WxPayNotifyRetDto {
            code: if success { "SUCCESS" } else { "FAIL" }.to_string(),
            message: (!success).then(|| "失败".to_string()),
        })
    }

    /// 回调解密取模块内首个 V3 密钥，不使用证书请求的商户配置组。
    async fn load_wx_notify_key(&self) -> Result<String, AppError> {
        app_setting::Entity::find_active()
            .filter(app_setting::Column::Module.eq(WX_PAY_MODULE))
            .filter(app_setting::Column::Key.eq("MchApiV3Key"))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .map(|item| item.value)
            .ok_or_else(|| AppError::internal("微信通知缺少 MchApiV3Key"))
    }

    /// 微信退款模板入口；订单号与金额保持源模板占位，未挂载 HTTP 路由。
    pub async fn wx_pay_refund(&self) -> Result<(), AppError> {
        let values = app_setting::Entity::find_active()
            .filter(app_setting::Column::Module.eq(WX_PAY_MODULE))
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;
        let first = |key: &str| {
            values
                .iter()
                .find(|item| item.key == key)
                .map(|item| item.value.as_str())
        };
        let (Some(app_id), Some(mch_id), Some(_), Some(_)) = (
            first("AppId"),
            first("MchId"),
            first("MchApiCertId"),
            first("MchApiCertKey"),
        ) else {
            return Ok(());
        };
        let settings = self.load_wx_request_settings(mch_id, app_id).await?;
        let request = json!({
            "out_refund_no": "",
            "transaction_id": "",
            "notify_url": "",
            "amount": { "refund": 1, "total": 1, "currency": "CNY" },
        });
        let url = format!("{WX_PAY_BASE_URL}/v3/refund/domestic/refunds");
        let body = self.wx_pay_http_post(&settings, &url, &request).await?;
        use common::json_helper::get_value_by_key;
        if !get_value_by_key(&body, "refund_id").is_some_and(|id| !id.is_empty()) {
            let code = get_value_by_key(&body, "code").unwrap_or_default();
            let message = get_value_by_key(&body, "message").unwrap_or_default();
            return Err(AppError::bad_request(format!("{code}:{message}")));
        }
        let _response: WxPayRefundResultDto = common::json_helper::json_to_object(&body)
            .map_err(|_| AppError::internal("微信退款响应格式无效"))?;
        Ok(())
    }

    /// 微信退款查询模板入口；源模板的商户退款单号为空。
    pub async fn wx_pay_refund_query(&self) -> Result<(), AppError> {
        let merchant = app_setting::Entity::find_active()
            .filter(app_setting::Column::Module.eq(WX_PAY_MODULE))
            .filter(app_setting::Column::Key.eq("MchId"))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;
        let Some(merchant) = merchant else {
            return Ok(());
        };
        let settings = self.load_wx_request_settings(&merchant.value, "").await?;
        let url = format!("{WX_PAY_BASE_URL}/v3/refund/domestic/refunds/");
        let body = self.wx_pay_http_get(&settings, &url).await?;
        // 源模板解析状态后没有进一步业务处理，也没有返回 DTO。
        let _response: WxPayRefundResultDto = common::json_helper::json_to_object(&body)
            .map_err(|_| AppError::internal("微信退款查询响应格式无效"))?;
        Ok(())
    }

    /// 支付宝退款模板入口；交易号、金额和退款识别号保持源占位值。
    pub async fn alipay_refund(&self) -> Result<(), AppError> {
        let settings = self.load_alipay_refund_settings().await?;
        self.execute_alipay_api(
            &settings,
            "alipay.trade.refund",
            json!({
                "trade_no": "", "refund_amount": "", "out_request_no": "",
            }),
            None,
            "UTF-8",
        )
        .await?;
        // SDK 业务错误与资金变动分支在源模板中均没有返回值或业务动作。
        Ok(())
    }

    /// 支付宝退款查询模板入口；交易号和退款识别号保持源占位值。
    pub async fn alipay_refund_query(&self) -> Result<(), AppError> {
        let settings = self.load_alipay_refund_settings().await?;
        self.execute_alipay_api(
            &settings,
            "alipay.trade.fastpay.refund.query",
            json!({
                "trade_no": "", "out_request_no": "",
            }),
            None,
            "UTF-8",
        )
        .await?;
        Ok(())
    }

    /// 向微信支付 API 发送 POST 请求，并对响应进行签名验证
    ///
    /// 构建带 WECHATPAY2-SHA256-RSA2048 鉴权头的请求，收到响应后验证微信平台签名的合法性
    async fn wx_pay_http_post(
        &self,
        settings: &WxPaySettings,
        url: &str,
        body: &Value,
    ) -> Result<String, AppError> {
        // 先序列化请求体，用于构造签名消息
        let body_str = body.to_string();
        let authorization = build_wx_pay_authorization(settings, "POST", url, &body_str)?;

        let response = self
            .context
            .http_client_factory
            .client()
            .post(url)
            .header("Authorization", authorization)
            .header("Accept", "application/json")
            .header("User-Agent", "rust_engine/1.0")
            .json(body)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("微信支付请求失败: {error}")))?;

        // 收集响应头，用于后续签名验证
        let response_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.to_string(), v.to_string()))
            })
            .collect();

        let response_body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("读取微信支付响应失败: {error}")))?;

        // 使用平台证书验证微信服务端的响应签名，防止中间人攻击
        let certificates = self.fetch_wx_pay_certificates(&settings.mch_id).await?;
        verify_wx_pay_response_sign(&response_headers, &response_body, &certificates)
            .map_err(|error| AppError::internal(format!("微信支付响应签名验证失败: {error}")))?;

        Ok(response_body)
    }

    /// 向微信支付 API 发送 GET 请求，并对响应进行签名验证
    async fn wx_pay_http_get(
        &self,
        settings: &WxPaySettings,
        url: &str,
    ) -> Result<String, AppError> {
        let authorization = build_wx_pay_authorization(settings, "GET", url, "")?;

        let response = self
            .context
            .http_client_factory
            .client()
            .get(url)
            .header("Authorization", authorization)
            .header("Accept", "application/json")
            .header("User-Agent", "rust_engine/1.0")
            .send()
            .await
            .map_err(|error| AppError::internal(format!("微信支付请求失败: {error}")))?;

        let response_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.to_string(), v.to_string()))
            })
            .collect();

        let response_body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("读取微信支付响应失败: {error}")))?;

        let certificates = self.fetch_wx_pay_certificates(&settings.mch_id).await?;
        verify_wx_pay_response_sign(&response_headers, &response_body, &certificates)
            .map_err(|error| AppError::internal(format!("微信支付响应签名验证失败: {error}")))?;

        Ok(response_body)
    }

    /// 从微信支付平台拉取证书列表并解密
    ///
    /// 证书用于验证微信响应和回调的签名，优先读取缓存并在未命中时加锁回源
    async fn fetch_wx_pay_certificates(
        &self,
        mch_id: &str,
    ) -> Result<WxPayCertificatesResponse, AppError> {
        let cache_key = Self::wx_pay_cert_cache_key(mch_id);
        if let Some(cached_value) = self.get_cached_json(&cache_key).await {
            return Ok(cached_value);
        }

        let lock_key = Self::wx_pay_cert_lock_key(mch_id);
        let guard = self
            .context
            .distributed_lock
            .lock(&lock_key, Duration::ZERO, 1)
            .await?;

        // 源服务使用默认六十秒锁，不主动续期；成功和失败均等待释放。
        let result = async {
            if let Some(cached_value) = self.get_cached_json(&cache_key).await {
                return Ok(cached_value);
            }

            // 平台证书缓存命中时不要求数据库中存在商户配置或 AppId。
            let settings = self.load_wx_request_settings(mch_id, "").await?;
            let certificates = self.request_wx_pay_certificates(&settings).await?;
            self.try_set_cached_json(
                &cache_key,
                &certificates,
                Duration::from_secs(WX_PAY_CERT_CACHE_TTL_SECONDS),
            )
            .await;

            // 写入助手忽略故障，证书用例通过再次读取确认取得缓存结果。
            self.get_cached_json(&cache_key)
                .await
                .ok_or_else(|| AppError::internal("证书获取失败"))
        }
        .await;

        guard.release().await;
        result
    }

    /// 从微信支付平台拉取证书列表并解密
    ///
    /// 证书用于验证微信响应和回调的签名，缓存未命中时才会实际请求微信接口
    async fn request_wx_pay_certificates(
        &self,
        settings: &WxPaySettings,
    ) -> Result<WxPayCertificatesResponse, AppError> {
        let url = format!("{}/v3/certificates", WX_PAY_BASE_URL);

        // 拉取证书接口本身也需要签名鉴权，请求体为空
        let authorization = build_wx_pay_authorization(settings, "GET", &url, "")?;

        let response = self
            .context
            .http_client_factory
            .client()
            .get(&url)
            .header("Authorization", authorization)
            .header("Accept", "application/json")
            .header("User-Agent", "rust_engine/1.0")
            .send()
            .await
            .map_err(|error| AppError::internal(format!("获取微信支付证书请求失败: {error}")))?;

        let response_headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.to_string(), value.to_string()))
            })
            .collect();

        let response_body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("读取微信支付证书响应失败: {error}")))?;

        let mut cert_resp: WxPayCertificatesResponse =
            common::json_helper::json_to_object(&response_body).map_err(|error| {
                AppError::internal(format!("解析微信支付证书响应失败: {error}"))
            })?;

        // 证书密文使用 API V3 密钥通过 AES-256-GCM 加密，保留原元数据。
        for entry in &mut cert_resp.data {
            entry.certificate = Some(decrypt_wx_pay_cert(entry, &settings.mch_api_v3_key)?);
        }

        // 证书下载响应使用本次解密所得的平台证书验签，成功后调用方才可写入缓存。
        verify_wx_pay_response_sign(&response_headers, &response_body, &cert_resp).map_err(
            |error| AppError::internal(format!("微信支付证书响应签名验证失败: {error}")),
        )?;

        Ok(cert_resp)
    }

    /// 将微信支付成功通知落库到订单
    ///
    /// 已支付的订单会直接跳过，防止重复处理
    async fn apply_wx_pay_transaction(
        &self,
        transaction: &WxPayNotifyTransactionDto,
    ) -> Result<bool, AppError> {
        let Some(order_no) = transaction.out_trade_no.as_deref() else {
            return Ok(false);
        };
        let Some(order_model) = self.load_payment_order(order_no).await? else {
            return Ok(false);
        };

        // 幂等检查：已标记支付成功则跳过
        if order_model.pay_state {
            return Ok(true);
        }

        // 微信支付金额单位为分，转回元存储
        let amount = transaction
            .amount
            .as_ref()
            .ok_or_else(|| AppError::internal("微信交易通知缺少金额"))?;
        let pay_price = Decimal::from(amount.total) / Decimal::from(100);
        let paid_at = chrono::DateTime::parse_from_rfc3339(&transaction.success_time)
            .map_err(|_| AppError::internal("微信交易通知支付时间格式无效"))?
            .to_utc();
        let mut active = order_model.into_active_model();
        active.pay_price = Set(Some(pay_price));
        active.serial_no = Set(transaction.transaction_id.clone());
        active.pay_state = Set(true);
        active.pay_time = Set(Some(paid_at.into()));
        active.pay_type = Set("微信支付".to_string());
        active.state = Set("已支付".to_string());
        active
            .update_versioned(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("更新微信支付订单状态失败: {error}")))?;

        Ok(true)
    }
}

/// 将 Decimal 金额转换为微信支付所需的整数分
///
/// 对应 Convert.ToInt32(decimal)：中点取偶数，超出范围返回内部错误。
fn decimal_to_fen(price: Decimal) -> Result<i32, AppError> {
    let fen = price
        .checked_mul(Decimal::from(100))
        .ok_or_else(|| AppError::internal("金额转换分溢出"))?
        .round()
        .to_string()
        .parse::<i32>()
        .map_err(|error| AppError::internal(format!("金额转换分失败: {error}")))?;

    Ok(fen)
}

/// 生成用于微信支付签名的随机字符串
///
/// 使用系统随机源生成 16 字节随机数，以 Base64 URL 安全无填充方式编码，确保字符集合法
fn generate_nonce() -> String {
    use rsa::rand_core::{OsRng, RngCore};

    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// 构建微信支付请求的 Authorization 鉴权头
///
/// 使用 WECHATPAY2-SHA256-RSA2048 方案，签名消息格式为：
/// HTTP方法\n请求路径\n时间戳\n随机串\n请求体\n
fn build_wx_pay_authorization(
    settings: &WxPaySettings,
    method: &str,
    url: &str,
    body: &str,
) -> Result<String, AppError> {
    let time_stamp = Utc::now().timestamp();
    let nonce_str = generate_nonce();

    // 签名消息中只需要 URL 路径部分，去掉域名前缀
    let url_path = url
        .trim_start_matches("https://api.mch.weixin.qq.com")
        .trim_start_matches("http://api.mch.weixin.qq.com");

    let message = format!(
        "{}\n{}\n{}\n{}\n{}\n",
        method, url_path, time_stamp, nonce_str, body
    );
    let signature = wx_pay_rsa_sign(&settings.mch_api_cert_key, &message)?;

    Ok(format!(
        "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",timestamp=\"{}\",serial_no=\"{}\",signature=\"{}\"",
        settings.mch_id, nonce_str, time_stamp, settings.mch_api_cert_id, signature
    ))
}

/// 使用商户 API 证书私钥对消息进行 RSA-PKCS1v15-SHA256 签名
///
/// 兼容 PKCS1（BEGIN RSA PRIVATE KEY）和 PKCS8（BEGIN PRIVATE KEY）两种 PEM 格式
fn wx_pay_rsa_sign(private_key_pem: &str, message: &str) -> Result<String, AppError> {
    use common::crypto_helper::{
        RsaHashAlgorithm, RsaSignaturePadding, StringEncoding, rsa_sign_data,
    };

    rsa_sign_data(
        private_key_pem,
        message,
        StringEncoding::Base64,
        RsaHashAlgorithm::Sha256,
        RsaSignaturePadding::Pkcs1,
    )
    .map_err(|error| AppError::internal(format!("微信支付 RSA 签名失败: {error}")))
}

/// 使用 AES-256-GCM 解密微信支付回调中的资源密文
///
/// 密文、随机串、附加数据均来自回调通知的 resource 字段，解密密钥为 API V3 密钥
fn aes_gcm_decrypt_wx(
    resource: &application_model::pay::models::WxPayNotifyResource,
    api_v3_key: &str,
) -> Result<String, AppError> {
    use common::crypto_helper::{StringEncoding, aes_gcm_decrypt};

    aes_gcm_decrypt(
        &resource.ciphertext,
        api_v3_key,
        &resource.nonce,
        Some(&resource.associated_data),
        StringEncoding::Base64,
    )
    .map_err(|error| AppError::internal(format!("解密微信支付通知密文失败: {error}")))
}

/// 使用 AES-256-GCM 解密微信支付平台证书密文，返回证书 PEM 文本
///
/// 证书在 /v3/certificates 接口中以加密形式下发，需用 API V3 密钥解密后才能使用
fn decrypt_wx_pay_cert(entry: &WxPayCertEntry, api_v3_key: &str) -> Result<String, AppError> {
    use common::crypto_helper::{StringEncoding, aes_gcm_decrypt};

    aes_gcm_decrypt(
        &entry.encrypt_certificate.ciphertext,
        api_v3_key,
        &entry.encrypt_certificate.nonce,
        Some(&entry.encrypt_certificate.associated_data),
        StringEncoding::Base64,
    )
    .map_err(|error| AppError::internal(format!("解密微信支付平台证书失败: {error}")))
}

/// 验证微信支付应答或回调的报文签名
///
/// 从响应头中提取时间戳、随机串、序列号和签名，依据序列号找到对应平台证书，
/// 从证书中提取 RSA 公钥，使用 PKCS1v15-SHA256 方案验证签名合法性
///
fn verify_wx_pay_response_sign(
    headers: &HashMap<String, String>,
    body: &str,
    certificates: &WxPayCertificatesResponse,
) -> Result<(), String> {
    use rsa::sha2::Sha256;
    use rsa::{RsaPublicKey, pkcs1v15::VerifyingKey, pkcs8::DecodePublicKey, signature::Verifier};
    use x509_cert::{
        Certificate,
        der::{Decode, Encode},
    };

    // 微信响应头名称不区分大小写，通过转小写进行不敏感匹配
    let header_lookup = |key: &str| -> Option<String> {
        let lower = key.to_lowercase();
        headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == lower)
            .map(|(_, v)| v.clone())
    };

    let nonce = header_lookup("Wechatpay-Nonce")
        .ok_or_else(|| "缺少 Wechatpay-Nonce 响应头".to_string())?;
    let timestamp = header_lookup("Wechatpay-Timestamp")
        .ok_or_else(|| "缺少 Wechatpay-Timestamp 响应头".to_string())?;
    let serial = header_lookup("Wechatpay-Serial")
        .ok_or_else(|| "缺少 Wechatpay-Serial 响应头".to_string())?;
    let signature_b64 = header_lookup("Wechatpay-Signature")
        .ok_or_else(|| "缺少 Wechatpay-Signature 响应头".to_string())?;

    // 根据序列号找到对应的平台证书 PEM
    let cert_pem = certificates
        .data
        .iter()
        .find(|entry| entry.serial_no == serial)
        .and_then(|entry| entry.certificate.as_deref())
        .ok_or_else(|| format!("未找到序列号 {} 对应的平台证书", serial))?;

    // 解析 X.509 证书，从中提取 SubjectPublicKeyInfo 并转为 RSA 公钥
    let cert_der =
        pem::parse(cert_pem).map_err(|error| format!("解析平台证书 PEM 失败: {error}"))?;
    let cert = Certificate::from_der(cert_der.contents())
        .map_err(|error| format!("解析平台证书 DER 失败: {error}"))?;
    let spki_der = cert
        .tbs_certificate()
        .subject_public_key_info()
        .to_der()
        .map_err(|error| format!("导出公钥 DER 失败: {error}"))?;
    let public_key = RsaPublicKey::from_public_key_der(&spki_der)
        .map_err(|error| format!("解析平台证书公钥失败: {error}"))?;

    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(&signature_b64)
        .map_err(|error| format!("解码签名 Base64 失败: {error}"))?;
    let signature = rsa::pkcs1v15::Signature::try_from(signature_bytes.as_slice())
        .map_err(|error| format!("解析签名失败: {error}"))?;

    // 签名消息格式：时间戳\n随机串\n响应体\n
    let message = format!("{}\n{}\n{}\n", timestamp, nonce, body);
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_| "微信支付响应签名验证失败".to_string())
}

/// 从微信支付 API 返回的 JSON 文本中提取指定字段值
///
/// 字段不存在时返回 code:message；显式 null 和空字符串按源 JSON 助手保留为空串。
fn extract_json_field(json_str: &str, field: &str) -> Result<String, AppError> {
    use common::json_helper::get_value_by_key;
    get_value_by_key(json_str, field).ok_or_else(|| {
        let code = get_value_by_key(json_str, "code").unwrap_or_default();
        let message = get_value_by_key(json_str, "message").unwrap_or_default();
        AppError::bad_request(format!("{code}:{message}"))
    })
}
