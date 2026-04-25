use repository::extensions::SoftDeleteQueryExt;
use std::{collections::HashMap, future::Future, str::FromStr, time::Duration};

use base64::Engine;
use chrono::{FixedOffset, NaiveDateTime, Utc};
use distributed_cache::{CacheEntryOptions, DistributedCacheExt};
use distributed_lock::{LockError, LockGuard};
use repository::{app_setting, order, user_bind_external};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    sha2::Sha256,
    signature::{RandomizedSigner, SignatureEncoding, Verifier},
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    prelude::Decimal,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use source_generator_runtime::retry;
use url::form_urlencoded;

use crate::write_support::write_data_update_log;
use application_interface::{
    context::{CurrentUser, PayContext},
    error::AppError,
};
use application_model::pay::models::{
    AliPayRefundRequest, AliPayRefundResultDto, WxPayAppDto, WxPayJsapiDto, WxPayNotifyDto,
    WxPayNotifyRetDto, WxPayRefundAmountDto, WxPayRefundRequest, WxPayRefundResultDto,
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
        format!("pay:wx:certificates:{mch_id}")
    }

    /// 证书互斥锁键
    fn wx_pay_cert_lock_key(mch_id: &str) -> String {
        format!("pay:wx:certificates:lock:{mch_id}")
    }

    /// 预支付结果缓存键
    fn wx_pay_create_cache_key(scene: &str, order_no: &str) -> String {
        format!("pay:wx:create:{scene}:{order_no}")
    }

    /// 预支付结果互斥锁键
    fn wx_pay_create_lock_key(scene: &str, order_no: &str) -> String {
        format!("pay:wx:create:lock:{scene}:{order_no}")
    }

    /// 读取 JSON 缓存
    async fn get_cached_json<T>(&self, key: &str) -> Result<Option<T>, AppError>
    where
        T: DeserializeOwned,
    {
        let Some(cache) = self.context.cache else {
            return Ok(None);
        };

        let Some(cached_value) = cache.get_string(key).await? else {
            return Ok(None);
        };

        match serde_json::from_str::<T>(&cached_value) {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                tracing::warn!(target: "pay.cache", cache_key = %key, error = %error, "支付缓存反序列化失败，按缓存未命中处理");
                let _ = cache.remove(key).await;
                Ok(None)
            }
        }
    }

    /// 尝试写入 JSON 缓存
    async fn try_set_cached_json<T>(&self, key: &str, value: &T, ttl: Duration)
    where
        T: Serialize,
    {
        if let Some(cache) = self.context.cache
            && let Ok(serialized) = serde_json::to_string(value)
        {
            let _ = cache
                .set_string_with_options(key, &serialized, CacheEntryOptions::relative(ttl))
                .await;
        }
    }

    /// 获取支付链路互斥锁
    async fn acquire_pay_lock(
        &self,
        key: &str,
        timeout_message: &str,
    ) -> Result<LockGuard, AppError> {
        self.context
            .distributed_lock
            .lock(key, Duration::from_secs(5), 1)
            .await
            .map_err(|error| match error {
                LockError::Timeout { .. } => AppError::conflict(timeout_message),
                _ => AppError::from(error),
            })
    }

    /// 带缓存和分布式锁的微信预支付结果获取
    async fn get_or_create_wx_cached_result<T, F, Fut>(
        &self,
        scene: &str,
        order_no: &str,
        producer: F,
    ) -> Result<T, AppError>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, AppError>>,
    {
        let cache_key = Self::wx_pay_create_cache_key(scene, order_no.trim());
        if let Some(cached_value) = self.get_cached_json(&cache_key).await? {
            return Ok(cached_value);
        }

        let lock_key = Self::wx_pay_create_lock_key(scene, order_no.trim());
        let _lock_guard = self
            .acquire_pay_lock(&lock_key, "支付请求处理中，请稍后重试")
            .await?;

        if let Some(cached_value) = self.get_cached_json(&cache_key).await? {
            return Ok(cached_value);
        }

        let result = producer().await?;
        self.try_set_cached_json(
            &cache_key,
            &result,
            Duration::from_secs(WX_PAY_CREATE_CACHE_TTL_SECONDS),
        )
        .await;

        Ok(result)
    }

    /// 创建支付宝 PC 支付地址
    ///
    /// 读取订单与支付宝配置后，生成可直接跳转的网页支付链接
    pub async fn create_alipay_pc(
        &self,
        order_no: &str,
        notify_url: &str,
        return_url: Option<&str>,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        let order_model = self.load_payable_order(order_no, current_user).await?;
        let settings = self.load_alipay_web_settings(None).await?;
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
            None,
        )
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
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        let order_model = self.load_payable_order(order_no, current_user).await?;
        let settings = self.load_alipay_web_settings(None).await?;
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
            quit_url,
        )
    }

    /// 创建支付宝小程序支付交易号
    ///
    /// 读取订单 支付宝小程序配置与用户绑定 OpenId 后 创建服务端支付单并返回 trade_no
    pub async fn create_alipay_mini_app(
        &self,
        order_no: &str,
        notify_url: &str,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        let order_model = self.load_payable_order(order_no, current_user).await?;
        let settings = self.load_alipay_mini_app_settings(None).await?;
        // 支付宝小程序场景要求传入买家标识，对应本地绑定表中的 OpenId
        let buyer_id = self
            .load_alipay_mini_app_buyer_id(current_user.user_id, &settings.app_id)
            .await?;

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
            )
            .await?;

        extract_alipay_trade_no(&response)
    }

    /// 处理支付宝异步通知
    ///
    /// 校验支付宝签名并在支付成功后更新订单支付状态
    pub async fn process_alipay_notify(
        &self,
        parameters: HashMap<String, String>,
    ) -> Result<String, AppError> {
        if parameters.is_empty() {
            return Err(AppError::bad_request("支付宝通知参数不可以为空"));
        }

        let app_id = parameters
            .get("app_id")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::bad_request("缺少 app_id"))?;
        let settings = self.load_alipay_web_settings(Some(app_id)).await?;
        verify_alipay_notify_signature(&parameters, &settings.alipay_public_key)?;

        let trade_status = parameters
            .get("trade_status")
            .map(String::as_str)
            .unwrap_or_default();

        if trade_status != "TRADE_SUCCESS" && trade_status != "TRADE_FINISHED" {
            return Ok("success".to_string());
        }

        let order_no = parameters
            .get("out_trade_no")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::bad_request("缺少 out_trade_no"))?;

        let order_model = order::Entity::find()
            .filter(order::Column::OrderNo.eq(order_no))
            .not_deleted(order::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .ok_or_else(|| AppError::bad_request("订单不存在"))?;

        if order_model.pay_state {
            return Ok("success".to_string());
        }

        let total_amount = parameters
            .get("total_amount")
            .map(String::as_str)
            .ok_or_else(|| AppError::bad_request("缺少 total_amount"))?;
        let trade_no = parameters
            .get("trade_no")
            .cloned()
            .ok_or_else(|| AppError::bad_request("缺少 trade_no"))?;
        let paid_at = parse_alipay_paid_time(parameters.get("gmt_payment").map(String::as_str))?;
        let pay_price = Decimal::from_str(total_amount)
            .map_err(|error| AppError::bad_request(format!("金额格式无效: {error}")))?;

        if pay_price != order_model.price {
            return Err(AppError::bad_request("支付金额与订单金额不一致"));
        }

        // 记录支付回调写入前的订单状态
        let before_log = json!({
            "id": order_model.id,
            "order_no": order_model.order_no,
            "pay_state": order_model.pay_state,
            "state": order_model.state,
            "serial_no": order_model.serial_no,
            "pay_type": order_model.pay_type,
            "pay_price": order_model.pay_price.map(|item| item.to_string()),
            "pay_time": order_model.pay_time.map(|item| item.to_rfc3339()),
        });
        let mut active = order_model.into_active_model();
        active.pay_price = Set(Some(pay_price));
        active.serial_no = Set(Some(trade_no));
        active.pay_state = Set(true);
        active.pay_time = Set(Some(paid_at.into()));
        active.pay_type = Set("支付宝".to_string());
        active.state = Set("已支付".to_string());
        let updated = active
            .update(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("更新订单支付状态失败: {error}")))?;

        // 记录支付回调写入后的订单状态
        write_data_update_log(
            self.context.app_context(),
            "Order",
            updated.id,
            "pay_notify",
            Some(before_log),
            Some(json!({
                "id": updated.id,
                "order_no": updated.order_no,
                "pay_state": updated.pay_state,
                "state": updated.state,
                "serial_no": updated.serial_no,
                "pay_type": updated.pay_type,
                "pay_price": updated.pay_price.map(|item| item.to_string()),
                "pay_time": updated.pay_time.map(|item| item.to_rfc3339()),
            })),
            None,
        )
        .await?;

        Ok("success".to_string())
    }

    /// 加载可支付订单
    ///
    /// 同时校验订单归属、删除状态与当前支付状态
    async fn load_payable_order(
        &self,
        order_no: &str,
        current_user: &CurrentUser,
    ) -> Result<order::Model, AppError> {
        let trimmed_order_no = order_no.trim();

        if trimmed_order_no.is_empty() {
            return Err(AppError::bad_request("订单号不可以为空"));
        }

        let order_model = order::Entity::find()
            .filter(order::Column::OrderNo.eq(trimmed_order_no))
            .not_deleted(order::Column::DeleteTime)
            .filter(order::Column::CreateUserId.eq(current_user.user_id))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .ok_or_else(|| AppError::bad_request("订单不存在或无权访问"))?;

        if order_model.pay_state || order_model.state != "待支付" {
            return Err(AppError::bad_request("当前订单状态不允许发起支付"));
        }

        Ok(order_model)
    }

    /// 加载当前用户可访问的订单
    async fn load_owned_order(
        &self,
        order_no: &str,
        current_user: &CurrentUser,
    ) -> Result<order::Model, AppError> {
        let trimmed_order_no = order_no.trim();

        if trimmed_order_no.is_empty() {
            return Err(AppError::bad_request("订单号不可以为空"));
        }

        order::Entity::find()
            .filter(order::Column::OrderNo.eq(trimmed_order_no))
            .not_deleted(order::Column::DeleteTime)
            .filter(order::Column::CreateUserId.eq(current_user.user_id))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .ok_or_else(|| AppError::bad_request("订单不存在或无权访问"))
    }

    /// 加载可退款订单
    async fn load_refundable_order(
        &self,
        order_no: &str,
        current_user: &CurrentUser,
    ) -> Result<order::Model, AppError> {
        let order_model = self.load_owned_order(order_no, current_user).await?;

        if !order_model.pay_state {
            return Err(AppError::bad_request("当前订单未支付，无法退款"));
        }

        Ok(order_model)
    }

    /// 加载支付宝网页支付配置
    ///
    /// 支持按应用标识匹配指定配置组，未传时默认取第一组有效配置
    async fn load_alipay_web_settings(
        &self,
        app_id: Option<&str>,
    ) -> Result<AliPayWebSettings, AppError> {
        self.load_alipay_settings(ALIPAY_WEB_MODULE, "支付宝网页支付", app_id)
            .await
    }

    /// 加载支付宝小程序支付配置
    ///
    /// 复用统一支付宝配置读取逻辑，并切换到 AliPayMiniApp 模块
    async fn load_alipay_mini_app_settings(
        &self,
        app_id: Option<&str>,
    ) -> Result<AliPayWebSettings, AppError> {
        self.load_alipay_settings(ALIPAY_MINI_APP_MODULE, "支付宝小程序支付", app_id)
            .await
    }

    /// 按模块加载支付宝配置
    ///
    /// 按模块名与可选 AppId 读取同组配置项并组装为统一配置对象
    async fn load_alipay_settings(
        &self,
        module: &str,
        scene_name: &str,
        app_id: Option<&str>,
    ) -> Result<AliPayWebSettings, AppError> {
        let models = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(module))
            .not_deleted(app_setting::Column::DeleteTime)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        if models.is_empty() {
            return Err(AppError::bad_request(format!("未配置{scene_name}参数")));
        }

        let target_group_id = if let Some(app_id) = app_id {
            models
                .iter()
                .find(|item| item.key == "AppId" && item.value == app_id)
                .map(|item| item.group_id)
                .ok_or_else(|| AppError::bad_request(format!("未找到匹配的{scene_name}应用配置")))?
        } else {
            models
                .iter()
                .find(|item| item.key == "AppId")
                .map(|item| item.group_id)
                .ok_or_else(|| AppError::bad_request(format!("缺少{scene_name} AppId 配置")))?
        };

        let group_settings = models
            .into_iter()
            .filter(|item| item.group_id == target_group_id)
            .collect::<Vec<_>>();

        Ok(AliPayWebSettings {
            app_id: find_setting_value(&group_settings, "AppId")?,
            app_private_key: normalize_private_key_pem(&find_setting_value(
                &group_settings,
                "AppPrivateKey",
            )?),
            alipay_public_key: normalize_public_key_pem(&find_setting_value(
                &group_settings,
                "AliPayPublicKey",
            )?),
            gateway_url: ALIPAY_GATEWAY_URL.to_string(),
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
    ) -> Result<Value, AppError> {
        let mut parameters = vec![
            ("app_id".to_string(), settings.app_id.clone()),
            ("biz_content".to_string(), biz_content.to_string()),
            ("charset".to_string(), "UTF-8".to_string()),
            ("format".to_string(), "json".to_string()),
            ("method".to_string(), method.to_string()),
            ("sign_type".to_string(), "RSA2".to_string()),
            (
                "timestamp".to_string(),
                Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ),
            ("version".to_string(), "1.0".to_string()),
        ];

        if let Some(notify_url) = notify_url.map(str::trim).filter(|value| !value.is_empty()) {
            parameters.push(("notify_url".to_string(), notify_url.to_string()));
        }

        // 与网页支付相同，先按字典序拼接待签名内容
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
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("调用支付宝接口失败: {error}")))?;

        let response_text = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("读取支付宝接口响应失败: {error}")))?;

        // 支付宝开放接口统一返回 JSON 包装体，这里直接保留原始结构供上层提取字段
        serde_json::from_str::<Value>(&response_text).map_err(|error| {
            AppError::internal(format!(
                "解析支付宝接口响应失败: {error}, 原始响应: {response_text}"
            ))
        })
    }

    /// 加载支付宝小程序买家标识
    ///
    /// 从用户外部绑定表中读取当前用户在支付宝小程序下的 OpenId
    async fn load_alipay_mini_app_buyer_id(
        &self,
        user_id: i64,
        app_id: &str,
    ) -> Result<String, AppError> {
        let buyer_id = user_bind_external::Entity::find()
            .filter(user_bind_external::Column::UserId.eq(user_id))
            .filter(user_bind_external::Column::AppName.eq(ALIPAY_MINI_APP_MODULE))
            .filter(user_bind_external::Column::AppId.eq(app_id))
            .not_deleted(user_bind_external::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .map(|item| item.open_id)
            .filter(|item| !item.trim().is_empty())
            .ok_or_else(|| AppError::bad_request("当前用户未绑定支付宝小程序 OpenId"))?;

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
        quit_url: Option<&str>,
    ) -> Result<String, AppError> {
        if notify_url.trim().is_empty() {
            return Err(AppError::bad_request("通知地址不可以为空"));
        }

        let mut parameters = vec![
            ("app_id".to_string(), settings.app_id.clone()),
            ("biz_content".to_string(), biz_content.to_string()),
            ("charset".to_string(), "UTF-8".to_string()),
            ("format".to_string(), "json".to_string()),
            ("method".to_string(), method.to_string()),
            ("notify_url".to_string(), notify_url.to_string()),
            ("sign_type".to_string(), "RSA2".to_string()),
            (
                "timestamp".to_string(),
                Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ),
            ("version".to_string(), "1.0".to_string()),
        ];

        if let Some(value) = return_url.filter(|value| !value.trim().is_empty()) {
            parameters.push(("return_url".to_string(), value.to_string()));
        }

        if let Some(value) = quit_url.filter(|value| !value.trim().is_empty()) {
            parameters.push(("quit_url".to_string(), value.to_string()));
        }

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

/// 查找配置值
fn find_setting_value(models: &[app_setting::Model], key: &str) -> Result<String, AppError> {
    models
        .iter()
        .find(|item| item.key == key)
        .map(|item| item.value.clone())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::bad_request(format!("缺少支付配置项 {key}")))
}

/// 构建订单标题
fn build_order_title(order_model: &order::Model) -> String {
    format!("{}交易", order_model.create_time.format("%Y%m%d%H%M"))
}

/// 规范化私钥 PEM 文本
fn normalize_private_key_pem(value: &str) -> String {
    if value.contains("BEGIN") {
        return value.to_string();
    }

    format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        wrap_pem_body(value)
    )
}

/// 规范化公钥 PEM 文本
fn normalize_public_key_pem(value: &str) -> String {
    if value.contains("BEGIN") {
        return value.to_string();
    }

    format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
        wrap_pem_body(value)
    )
}

/// 按 PEM 固定宽度包装内容
fn wrap_pem_body(value: &str) -> String {
    let compact = value.lines().collect::<String>();
    compact
        .as_bytes()
        .chunks(64)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect::<Vec<_>>()
        .join("\n")
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
    let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem)
        .map_err(|error| AppError::internal(format!("解析支付宝私钥失败: {error}")))?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let mut rng = rsa::rand_core::OsRng;
    let signature = signing_key.sign_with_rng(&mut rng, content.as_bytes());

    Ok(base64::engine::general_purpose::STANDARD.encode(signature.to_vec()))
}

/// 校验支付宝异步通知签名
fn verify_alipay_notify_signature(
    parameters: &HashMap<String, String>,
    public_key_pem: &str,
) -> Result<(), AppError> {
    let signature_text = parameters
        .get("sign")
        .map(String::as_str)
        .ok_or_else(|| AppError::bad_request("缺少 sign"))?;
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_text)
        .map_err(|error| AppError::bad_request(format!("支付宝签名格式无效: {error}")))?;
    let signature = Signature::try_from(signature_bytes.as_slice())
        .map_err(|error| AppError::bad_request(format!("支付宝签名解析失败: {error}")))?;
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .map_err(|error| AppError::internal(format!("解析支付宝公钥失败: {error}")))?;
    let verify_key = VerifyingKey::<Sha256>::new(public_key);

    let mut pairs = parameters
        .iter()
        .filter(|(key, value)| *key != "sign" && *key != "sign_type" && !value.is_empty())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();

    pairs.sort_by(|left, right| left.0.cmp(&right.0));
    let content = build_sign_content(&pairs);

    verify_key
        .verify(content.as_bytes(), &signature)
        .map_err(|_| AppError::unauthorized("支付宝签名校验失败"))
}

/// 解析支付宝支付完成时间
fn parse_alipay_paid_time(value: Option<&str>) -> Result<chrono::DateTime<Utc>, AppError> {
    let value = value
        .filter(|item| !item.trim().is_empty())
        .ok_or_else(|| AppError::bad_request("缺少 gmt_payment"))?;
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map_err(|error| AppError::bad_request(format!("支付时间格式无效: {error}")))?;
    let offset =
        FixedOffset::east_opt(8 * 3600).ok_or_else(|| AppError::internal("创建东八区时区失败"))?;

    naive
        .and_local_timezone(offset)
        .single()
        .map(|item| item.to_utc())
        .ok_or_else(|| AppError::bad_request("支付时间无法转换为唯一时刻"))
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

/// 将支付宝退款负载映射为统一返回模型
fn map_alipay_refund_result(payload: &Value, status: Option<String>) -> AliPayRefundResultDto {
    AliPayRefundResultDto {
        trade_no: payload
            .get("trade_no")
            .and_then(Value::as_str)
            .map(str::to_string),
        out_trade_no: payload
            .get("out_trade_no")
            .and_then(Value::as_str)
            .map(str::to_string),
        out_request_no: payload
            .get("out_request_no")
            .and_then(Value::as_str)
            .map(str::to_string),
        refund_amount: payload
            .get("refund_fee")
            .and_then(Value::as_str)
            .map(str::to_string),
        total_amount: payload
            .get("total_amount")
            .and_then(Value::as_str)
            .map(str::to_string),
        fund_change: payload
            .get("fund_change")
            .and_then(Value::as_str)
            .map(str::to_string),
        status,
    }
}

/// 将微信退款响应映射为统一返回模型
fn map_wx_pay_refund_result(result: WxPayRefundResponse) -> WxPayRefundResultDto {
    WxPayRefundResultDto {
        out_refund_no: result.out_refund_no,
        out_trade_no: result.out_trade_no,
        refund_id: result.refund_id,
        status: result.status,
        transaction_id: result.transaction_id,
        success_time: result.success_time.map(|value| value.to_rfc3339()),
        amount: WxPayRefundAmountDto {
            currency: result.amount.currency,
            refund: result.amount.refund,
            total: result.amount.total,
        },
    }
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
#[derive(Debug, Deserialize)]
struct WxPayCertEntry {
    /// 证书序列号
    serial_no: String,

    /// 加密证书信息
    encrypt_certificate: WxPayEncryptedCert,
}

/// 微信支付加密证书信息
#[derive(Debug, Deserialize)]
struct WxPayEncryptedCert {
    /// 密文
    ciphertext: String,

    /// 附加数据
    associated_data: String,

    /// 随机串
    nonce: String,
}

/// 微信支付平台证书列表
#[derive(Debug, Deserialize)]
struct WxPayCertificatesResponse {
    /// 证书列表
    data: Vec<WxPayCertEntry>,
}

/// 微信支付支付成功通知资源明文
#[derive(Debug, Deserialize)]
struct WxPayTransactionResource {
    /// 订单号
    out_trade_no: String,

    /// 微信支付流水号
    transaction_id: String,

    /// 交易状态
    trade_state: String,

    /// 支付完成时间（RFC3339）
    success_time: Option<chrono::DateTime<Utc>>,

    /// 金额信息
    amount: WxPayTransactionAmount,
}

/// 微信支付金额信息
#[derive(Debug, Deserialize)]
struct WxPayTransactionAmount {
    /// 订单金额，单位分
    total: i32,
}

/// 微信退款响应金额信息
#[derive(Debug, Deserialize)]
struct WxPayRefundResponseAmount {
    /// 币种
    currency: String,

    /// 退款金额（分）
    refund: i32,

    /// 原订单金额（分）
    total: i32,
}

/// 微信退款响应
#[derive(Debug, Deserialize)]
struct WxPayRefundResponse {
    /// 商户退款单号
    out_refund_no: String,

    /// 商户订单号
    out_trade_no: String,

    /// 微信退款单号
    refund_id: String,

    /// 退款状态
    status: String,

    /// 微信支付流水号
    transaction_id: Option<String>,

    /// 退款成功时间
    success_time: Option<chrono::DateTime<Utc>>,

    /// 金额信息
    amount: WxPayRefundResponseAmount,
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
        current_user: &CurrentUser,
    ) -> Result<WxPayJsapiDto, AppError> {
        self.get_or_create_wx_cached_result("jsapi", order_no, || async move {
            let order_model = self.load_payable_order(order_no, current_user).await?;
            let settings = self.load_wx_pay_settings(None).await?;

            let price_fen = decimal_to_fen(order_model.price)?;
            let full_notify_url =
                format!("{}{}", notify_url.trim_end_matches('/'), settings.mch_id);
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
            let sign = wx_pay_rsa_sign(&settings.mch_api_cert_key, &message)?;

            Ok(WxPayJsapiDto {
                app_id: settings.app_id,
                package,
                nonce_str,
                time_stamp,
                sign,
            })
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
        current_user: &CurrentUser,
    ) -> Result<WxPayAppDto, AppError> {
        self.get_or_create_wx_cached_result("app", order_no, || async move {
            let order_model = self.load_payable_order(order_no, current_user).await?;
            let settings = self.load_wx_pay_settings(None).await?;

            let price_fen = decimal_to_fen(order_model.price)?;
            let full_notify_url =
                format!("{}{}", notify_url.trim_end_matches('/'), settings.mch_id);
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
            let sign = wx_pay_rsa_sign(&settings.mch_api_cert_key, &message)?;

            Ok(WxPayAppDto {
                app_id: settings.app_id,
                partner_id: settings.mch_id,
                prepay_id,
                nonce_str,
                time_stamp,
                sign,
            })
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
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        self.get_or_create_wx_cached_result("h5", order_no, || async move {
            let order_model = self.load_payable_order(order_no, current_user).await?;
            let settings = self.load_wx_pay_settings(None).await?;

            let price_fen = decimal_to_fen(order_model.price)?;
            let full_notify_url =
                format!("{}{}", notify_url.trim_end_matches('/'), settings.mch_id);
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
            extract_json_field(&result_json, "h5_url")
        })
        .await
    }

    /// 创建微信支付 PC 扫码支付链接
    ///
    /// 向微信支付下单并返回二维码链接，前端渲染为二维码后供用户扫码支付，有效期 2 小时
    pub async fn create_wx_pay_pc(
        &self,
        order_no: &str,
        notify_url: &str,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        self.get_or_create_wx_cached_result("pc", order_no, || async move {
            let order_model = self.load_payable_order(order_no, current_user).await?;
            let settings = self.load_wx_pay_settings(None).await?;

            let price_fen = decimal_to_fen(order_model.price)?;
            let full_notify_url =
                format!("{}{}", notify_url.trim_end_matches('/'), settings.mch_id);
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
            extract_json_field(&result_json, "code_url")
        })
        .await
    }

    /// 处理微信支付异步回调
    ///
    /// 依次完成签名验证、密文解密、订单状态更新，始终向微信返回 200 响应以避免重试
    pub async fn process_wx_pay_notify(
        &self,
        mch_id: &str,
        notify_dto: WxPayNotifyDto,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<WxPayNotifyRetDto, AppError> {
        let settings = self.load_wx_pay_settings(Some(mch_id)).await?;

        // 获取平台证书用于验证微信签名
        let certificates = self.fetch_wx_pay_certificates(&settings).await?;

        // 签名验证失败时仍返回 200，防止微信反复重推
        if let Err(error) =
            verify_wx_pay_response_sign(headers, body, &certificates, &settings.mch_api_v3_key)
        {
            tracing::error!(target: "pay.wx_pay.notify", mch_id = %mch_id, error = %error, "微信支付回调签名验证失败");

            return Ok(WxPayNotifyRetDto {
                code: "FAIL".to_string(),
                message: Some("签名验证失败".to_string()),
            });
        }

        let event_type = notify_dto.event_type.as_deref().unwrap_or_default();

        // 支付成功通知：解密资源密文并写入订单支付状态
        if event_type == "TRANSACTION.SUCCESS" {
            let plaintext = aes_gcm_decrypt_wx(&notify_dto.resource, &settings.mch_api_v3_key)?;
            let transaction: WxPayTransactionResource =
                serde_json::from_str(&plaintext).map_err(|error| {
                    AppError::internal(format!("解析微信支付通知明文失败: {error}"))
                })?;

            if transaction.trade_state == "SUCCESS" {
                self.apply_wx_pay_transaction(&transaction).await?;
            }
        }

        // 退款结果回调：业务方可在此扩展退款后的处理逻辑
        if notify_dto.resource.original_type.as_deref() == Some("refund") {
            tracing::info!(target: "pay.wx_pay.notify", mch_id = %mch_id, "收到微信支付退款回调");
        }

        Ok(WxPayNotifyRetDto {
            code: "SUCCESS".to_string(),
            message: None,
        })
    }

    /// 发起微信支付退款
    pub async fn wx_pay_refund(
        &self,
        request: WxPayRefundRequest,
        current_user: &CurrentUser,
    ) -> Result<WxPayRefundResultDto, AppError> {
        if request.out_refund_no.trim().is_empty() {
            return Err(AppError::bad_request("商户退款单号不可以为空"));
        }

        let order_model = self
            .load_refundable_order(&request.order_no, current_user)
            .await?;
        let settings = self.load_wx_pay_settings(None).await?;
        let refund_amount = parse_decimal_amount(&request.refund_amount, "refundAmount")?;
        let total_amount = order_model.pay_price.unwrap_or(order_model.price);
        let refund_fen = decimal_to_fen(refund_amount)?;
        let total_fen = decimal_to_fen(total_amount)?;

        if refund_fen <= 0 {
            return Err(AppError::bad_request("退款金额必须大于 0"));
        }

        if refund_fen > total_fen {
            return Err(AppError::bad_request("退款金额不可以大于原订单金额"));
        }

        let transaction_id = order_model
            .serial_no
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::bad_request("订单缺少微信支付流水号，无法发起退款"))?;

        let req_body = json!({
            "out_refund_no": request.out_refund_no.trim(),
            "transaction_id": transaction_id,
            "amount": {
                "refund": refund_fen,
                "total": total_fen,
                "currency": "CNY"
            }
        });

        let url = format!("{}/v3/refund/domestic/refunds", WX_PAY_BASE_URL);
        let result_json = self.wx_pay_http_post(&settings, &url, &req_body).await?;
        let result = serde_json::from_str::<WxPayRefundResponse>(&result_json)
            .map_err(|error| AppError::internal(format!("解析微信退款响应失败: {error}")))?;

        Ok(map_wx_pay_refund_result(result))
    }

    /// 查询微信支付退款状态
    pub async fn wx_pay_refund_query(
        &self,
        order_no: &str,
        out_refund_no: &str,
        current_user: &CurrentUser,
    ) -> Result<WxPayRefundResultDto, AppError> {
        if out_refund_no.trim().is_empty() {
            return Err(AppError::bad_request("商户退款单号不可以为空"));
        }

        let _ = self.load_owned_order(order_no, current_user).await?;
        let settings = self.load_wx_pay_settings(None).await?;
        let url = format!(
            "{}/v3/refund/domestic/refunds/{}",
            WX_PAY_BASE_URL,
            out_refund_no.trim()
        );
        let result_json = self.wx_pay_http_get(&settings, &url).await?;
        let result = serde_json::from_str::<WxPayRefundResponse>(&result_json)
            .map_err(|error| AppError::internal(format!("解析微信退款查询响应失败: {error}")))?;

        Ok(map_wx_pay_refund_result(result))
    }

    /// 发起支付宝退款
    pub async fn alipay_refund(
        &self,
        request: AliPayRefundRequest,
        current_user: &CurrentUser,
    ) -> Result<AliPayRefundResultDto, AppError> {
        if request.out_request_no.trim().is_empty() {
            return Err(AppError::bad_request("退款请求号不可以为空"));
        }

        let order_model = self
            .load_refundable_order(&request.order_no, current_user)
            .await?;
        let settings = self.load_alipay_web_settings(None).await?;
        let refund_amount = parse_decimal_amount(&request.refund_amount, "refundAmount")?;
        let total_amount = order_model.pay_price.unwrap_or(order_model.price);

        if refund_amount <= Decimal::ZERO {
            return Err(AppError::bad_request("退款金额必须大于 0"));
        }

        if refund_amount > total_amount {
            return Err(AppError::bad_request("退款金额不可以大于原订单金额"));
        }

        let response = self
            .execute_alipay_api(
                &settings,
                "alipay.trade.refund",
                json!({
                    "trade_no": order_model.serial_no,
                    "out_trade_no": order_model.order_no,
                    "refund_amount": request.refund_amount.trim(),
                    "out_request_no": request.out_request_no.trim(),
                }),
                None,
            )
            .await?;
        let payload = extract_alipay_success_payload(
            &response,
            "alipay_trade_refund_response",
            "支付宝退款失败",
        )?;

        Ok(map_alipay_refund_result(
            payload,
            Some(
                if payload.get("fund_change").and_then(Value::as_str) == Some("Y") {
                    "SUCCESS".to_string()
                } else {
                    "PROCESSING".to_string()
                },
            ),
        ))
    }

    /// 查询支付宝退款状态
    pub async fn alipay_refund_query(
        &self,
        order_no: &str,
        out_request_no: &str,
        current_user: &CurrentUser,
    ) -> Result<AliPayRefundResultDto, AppError> {
        if out_request_no.trim().is_empty() {
            return Err(AppError::bad_request("退款请求号不可以为空"));
        }

        let order_model = self.load_owned_order(order_no, current_user).await?;
        let settings = self.load_alipay_web_settings(None).await?;
        let response = self
            .execute_alipay_api(
                &settings,
                "alipay.trade.fastpay.refund.query",
                json!({
                    "out_trade_no": order_model.order_no,
                    "trade_no": order_model.serial_no,
                    "out_request_no": out_request_no.trim(),
                }),
                None,
            )
            .await?;
        let payload = extract_alipay_success_payload(
            &response,
            "alipay_trade_fastpay_refund_query_response",
            "支付宝退款查询失败",
        )?;

        Ok(map_alipay_refund_result(
            payload,
            payload
                .get("refund_status")
                .and_then(Value::as_str)
                .map(str::to_string),
        ))
    }

    /// 从数据库加载微信支付配置
    ///
    /// 传入 mch_id 则精确匹配对应商户配置，不传则取第一个可用商户配置
    async fn load_wx_pay_settings(&self, mch_id: Option<&str>) -> Result<WxPaySettings, AppError> {
        let models = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(WX_PAY_MODULE))
            .not_deleted(app_setting::Column::DeleteTime)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        if models.is_empty() {
            return Err(AppError::bad_request("未配置微信支付参数"));
        }

        // 多商户场景下通过 MchId 匹配同一 group_id 的配置组
        let target_group_id = if let Some(mch_id) = mch_id {
            models
                .iter()
                .find(|item| item.key == "MchId" && item.value == mch_id)
                .map(|item| item.group_id)
                .ok_or_else(|| AppError::bad_request("未找到匹配的微信支付商户配置"))?
        } else {
            models
                .iter()
                .find(|item| item.key == "MchId")
                .map(|item| item.group_id)
                .ok_or_else(|| AppError::bad_request("缺少微信支付商户号配置"))?
        };

        // 按 group_id 过滤出同一商户的全部配置项
        let group = models
            .into_iter()
            .filter(|item| item.group_id == target_group_id)
            .collect::<Vec<_>>();

        Ok(WxPaySettings {
            app_id: find_setting_value(&group, "AppId")?,
            mch_id: find_setting_value(&group, "MchId")?,
            mch_api_cert_id: find_setting_value(&group, "MchApiCertId")?,
            mch_api_cert_key: find_setting_value(&group, "MchApiCertKey")?,
            mch_api_v3_key: find_setting_value(&group, "MchApiV3Key")?,
        })
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
        let certificates = self.fetch_wx_pay_certificates(settings).await?;
        verify_wx_pay_response_sign(
            &response_headers,
            &response_body,
            &certificates,
            &settings.mch_api_v3_key,
        )
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

        let certificates = self.fetch_wx_pay_certificates(settings).await?;
        verify_wx_pay_response_sign(
            &response_headers,
            &response_body,
            &certificates,
            &settings.mch_api_v3_key,
        )
        .map_err(|error| AppError::internal(format!("微信支付响应签名验证失败: {error}")))?;

        Ok(response_body)
    }

    /// 从微信支付平台拉取证书列表并解密
    ///
    /// 证书用于验证微信响应和回调的签名，每次都从 API 实时拉取，不做本地缓存
    #[retry(max_attempts = 3, delay_ms = 200)]
    async fn fetch_wx_pay_certificates(
        &self,
        settings: &WxPaySettings,
    ) -> Result<Vec<(String, String)>, AppError> {
        let cache_key = Self::wx_pay_cert_cache_key(&settings.mch_id);
        if let Some(cached_value) = self.get_cached_json(&cache_key).await? {
            return Ok(cached_value);
        }

        let lock_key = Self::wx_pay_cert_lock_key(&settings.mch_id);
        let _lock_guard = self
            .acquire_pay_lock(&lock_key, "微信支付证书加载中，请稍后重试")
            .await?;

        if let Some(cached_value) = self.get_cached_json(&cache_key).await? {
            return Ok(cached_value);
        }

        let certificates = self.request_wx_pay_certificates(settings).await?;
        self.try_set_cached_json(
            &cache_key,
            &certificates,
            Duration::from_secs(WX_PAY_CERT_CACHE_TTL_SECONDS),
        )
        .await;

        Ok(certificates)
    }

    /// 从微信支付平台拉取证书列表并解密
    ///
    /// 证书用于验证微信响应和回调的签名，缓存未命中时才会实际请求微信接口
    #[retry(max_attempts = 3, delay_ms = 200)]
    async fn request_wx_pay_certificates(
        &self,
        settings: &WxPaySettings,
    ) -> Result<Vec<(String, String)>, AppError> {
        let url = format!("{}/v3/certificates", WX_PAY_BASE_URL);

        // 拉取证书接口本身也需要签名鉴权，请求体为空
        let authorization = build_wx_pay_authorization(settings, "GET", &url, "")?;

        let response_body = self
            .context
            .http_client_factory
            .client()
            .get(&url)
            .header("Authorization", authorization)
            .header("Accept", "application/json")
            .header("User-Agent", "rust_engine/1.0")
            .send()
            .await
            .map_err(|error| AppError::internal(format!("获取微信支付证书请求失败: {error}")))?
            .text()
            .await
            .map_err(|error| AppError::internal(format!("读取微信支付证书响应失败: {error}")))?;

        let cert_resp: WxPayCertificatesResponse = serde_json::from_str(&response_body)
            .map_err(|error| AppError::internal(format!("解析微信支付证书响应失败: {error}")))?;

        let mut result = Vec::new();

        // 证书密文使用 API V3 密钥通过 AES-256-GCM 加密，逐条解密后返回 (序列号, PEM 文本)
        for entry in cert_resp.data {
            let pem = decrypt_wx_pay_cert(&entry, &settings.mch_api_v3_key)?;
            result.push((entry.serial_no, pem));
        }

        Ok(result)
    }

    /// 将微信支付成功通知落库到订单
    ///
    /// 已支付的订单会直接跳过，防止重复处理
    async fn apply_wx_pay_transaction(
        &self,
        transaction: &WxPayTransactionResource,
    ) -> Result<(), AppError> {
        let order_model = order::Entity::find()
            .filter(order::Column::OrderNo.eq(&transaction.out_trade_no))
            .not_deleted(order::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(order_model) = order_model else {
            // 订单不存在时仅记录日志，不返回错误，避免微信重复推送
            tracing::warn!(
                target: "pay.wx_pay.notify",
                out_trade_no = %transaction.out_trade_no,
                "微信支付通知对应订单不存在"
            );

            return Ok(());
        };

        // 幂等检查：已标记支付成功则跳过
        if order_model.pay_state {
            return Ok(());
        }

        // 微信支付金额单位为分，转回元存储
        let pay_price =
            Decimal::from_str(&(transaction.amount.total as f64 / 100.0).to_string())
                .map_err(|error| AppError::internal(format!("转换微信支付金额失败: {error}")))?;
        let paid_at = transaction.success_time.unwrap_or_else(Utc::now);

        // 记录支付回调写入前的订单状态
        let before_log = json!({
            "id": order_model.id,
            "order_no": order_model.order_no,
            "pay_state": order_model.pay_state,
            "state": order_model.state,
            "serial_no": order_model.serial_no,
            "pay_type": order_model.pay_type,
            "pay_price": order_model.pay_price.map(|item| item.to_string()),
            "pay_time": order_model.pay_time.map(|item| item.to_rfc3339()),
        });
        let mut active = order_model.into_active_model();
        active.pay_price = Set(Some(pay_price));
        active.serial_no = Set(Some(transaction.transaction_id.clone()));
        active.pay_state = Set(true);
        active.pay_time = Set(Some(paid_at.into()));
        active.pay_type = Set("微信支付".to_string());
        active.state = Set("已支付".to_string());
        let updated = active
            .update(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("更新微信支付订单状态失败: {error}")))?;

        // 记录支付回调写入后的订单状态
        write_data_update_log(
            self.context.app_context(),
            "Order",
            updated.id,
            "pay_notify",
            Some(before_log),
            Some(json!({
                "id": updated.id,
                "order_no": updated.order_no,
                "pay_state": updated.pay_state,
                "state": updated.state,
                "serial_no": updated.serial_no,
                "pay_type": updated.pay_type,
                "pay_price": updated.pay_price.map(|item| item.to_string()),
                "pay_time": updated.pay_time.map(|item| item.to_rfc3339()),
            })),
            None,
        )
        .await?;

        Ok(())
    }
}

/// 将 Decimal 金额转换为微信支付所需的整数分
///
/// 微信支付 API 要求金额以分为单位传递，精度截断后四舍五入
fn decimal_to_fen(price: Decimal) -> Result<i32, AppError> {
    let fen = (price * Decimal::from(100))
        .round()
        .to_string()
        .parse::<i32>()
        .map_err(|error| AppError::internal(format!("金额转换分失败: {error}")))?;

    Ok(fen)
}

/// 解析金额字符串
fn parse_decimal_amount(value: &str, field_name: &str) -> Result<Decimal, AppError> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return Err(AppError::bad_request(format!("{field_name} 不可以为空")));
    }

    Decimal::from_str(trimmed)
        .map_err(|error| AppError::bad_request(format!("{field_name} 金额格式无效: {error}")))
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
    use rsa::sha2::Sha256;
    use rsa::{
        RsaPrivateKey, pkcs1::DecodeRsaPrivateKey, pkcs1v15::SigningKey, pkcs8::DecodePrivateKey,
        signature::Signer,
    };

    let pem = private_key_pem.trim();

    // 根据 PEM 头部内容区分 PKCS1 与 PKCS8 格式
    let private_key = if pem.contains("BEGIN RSA PRIVATE KEY") {
        RsaPrivateKey::from_pkcs1_pem(pem)
            .map_err(|error| AppError::internal(format!("解析微信支付私钥(PKCS1)失败: {error}")))?
    } else {
        RsaPrivateKey::from_pkcs8_pem(pem)
            .map_err(|error| AppError::internal(format!("解析微信支付私钥(PKCS8)失败: {error}")))?
    };

    let signing_key = SigningKey::<Sha256>::new(private_key);
    let signature = signing_key.sign(message.as_bytes());

    Ok(base64::engine::general_purpose::STANDARD.encode(signature.to_bytes().as_ref()))
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
        resource.associated_data.as_deref(),
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
/// `certificates` 为 (serial_no, cert_pem) 列表
fn verify_wx_pay_response_sign(
    headers: &HashMap<String, String>,
    body: &str,
    certificates: &[(String, String)],
    _api_v3_key: &str,
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
        .iter()
        .find(|(sn, _)| sn == &serial)
        .map(|(_, pem)| pem.as_str())
        .ok_or_else(|| format!("未找到序列号 {} 对应的平台证书", serial))?;

    // 解析 X.509 证书，从中提取 SubjectPublicKeyInfo 并转为 RSA 公钥
    let cert_der =
        pem::parse(cert_pem).map_err(|error| format!("解析平台证书 PEM 失败: {error}"))?;
    let cert = Certificate::from_der(cert_der.contents())
        .map_err(|error| format!("解析平台证书 DER 失败: {error}"))?;
    let spki_der = cert
        .tbs_certificate
        .subject_public_key_info
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
/// 字段不存在或为空时，将 code 和 message 字段拼装为错误信息返回
fn extract_json_field(json_str: &str, field: &str) -> Result<String, AppError> {
    let value: Value = serde_json::from_str(json_str)
        .map_err(|error| AppError::internal(format!("解析微信支付响应 JSON 失败: {error}")))?;

    value
        .get(field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            // 提取错误码和错误描述，拼装为可读的错误信息
            let code = value
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN");
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("未知错误");
            AppError::bad_request(format!("微信支付返回错误 {code}: {message}"))
        })
}
