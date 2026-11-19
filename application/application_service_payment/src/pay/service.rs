use repository::extensions::SoftDeleteEntityExt;
use repository_core::versioned::VersionedActiveModel;
use std::{collections::HashMap, future::Future, time::Duration};

use super::{
    alipay::{self, ALIPAY_GATEWAY_URL, AliPayWebSettings},
    wechat::{PrepayOrder, WxClient, WxPaySettings},
};
use chrono::Utc;
use distributed_cache::{CacheEntryOptions, DistributedCacheExt};
use repository::{app_setting, order, user_bind_external};
use sea_orm::{
    ActiveValue::Set, ColumnTrait, IntoActiveModel, QueryFilter, QuerySelect, prelude::Decimal,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;

use application_interface::error::AppError;
use application_model::pay::models::{WxPayAppDto, WxPayJsapiDto};
use application_runtime::context::PayContext;

/// 支付能力应用服务实现
///
/// 负责统一封装支付宝与微信支付的下单、签名校验与回调落库逻辑
/// 支付宝网页支付模块名
const ALIPAY_WEB_MODULE: &str = "AliPayWeb";

/// 支付宝小程序支付模块名
const ALIPAY_MINI_APP_MODULE: &str = "AliPayMiniApp";

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

    fn wx_client(&self) -> WxClient<'_> {
        WxClient {
            http: self.context.http_client_factory.client(),
            cache: self.context.cache,
            lock: self.context.distributed_lock,
        }
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

    /// H5 和 PC 链接缓存原始字符串；缓存读写故障直接传播。
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
        if cached.is_some() {
            return Ok(cached);
        }

        let result = producer().await?;
        if let Some(value) = &result {
            self.context
                .cache
                .set_string_with_options(cache_key, value, CacheEntryOptions::relative(ttl))
                .await?;
        }
        Ok(result)
    }

    async fn load_payment_order(&self, order_no: &str) -> Result<Option<order::Model>, AppError> {
        if order_no.trim().is_empty() {
            return Err(AppError::bad_request("订单号不能为空"));
        }
        let rows = order::Entity::find_active()
            .filter(order::Column::OrderNo.eq(order_no))
            .limit(2)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;
        unique(rows)
    }

    async fn payable_order(&self, order_no: &str) -> Result<Option<order::Model>, AppError> {
        let order = self
            .load_payment_order(order_no)
            .await?
            .filter(|o| !o.pay_state && o.state == "待支付");
        if order
            .as_ref()
            .is_some_and(|o| o.price <= Decimal::ZERO || o.price.round_dp(2) != o.price)
        {
            return Err(AppError::bad_request("订单支付金额无效"));
        }
        Ok(order)
    }

    async fn settings_group(
        &self,
        modules: &[&str],
        key: &str,
        value: Option<&str>,
    ) -> Result<Vec<app_setting::Model>, AppError> {
        let rows = app_setting::Entity::find_active()
            .filter(app_setting::Column::Module.is_in(modules.iter().copied()))
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;
        let mut matches = rows
            .iter()
            .filter(|r| r.key == key && value.is_none_or(|v| r.value == v));
        let selected = matches
            .next()
            .filter(|_| matches.next().is_none())
            .ok_or_else(|| AppError::bad_request("支付配置缺失或存在多个匹配的配置组"))?;
        let group = selected.group_id;
        let module = selected.module.clone();
        Ok(rows
            .into_iter()
            .filter(|r| r.group_id == group && r.module == module)
            .collect())
    }

    async fn wx_settings(&self, merchant: Option<&str>) -> Result<WxPaySettings, AppError> {
        let group = self
            .settings_group(&[WX_PAY_MODULE], "MchId", merchant)
            .await?;
        Ok(WxPaySettings {
            app_id: setting(&group, "AppId")?,
            mch_id: setting(&group, "MchId")?,
            mch_api_cert_id: setting(&group, "MchApiCertId")?,
            mch_api_cert_key: setting(&group, "MchApiCertKey")?,
            mch_api_v3_key: setting(&group, "MchApiV3Key")?,
        })
    }

    async fn ali_settings(
        &self,
        modules: &[&str],
        app: Option<&str>,
    ) -> Result<AliPayWebSettings, AppError> {
        let group = self.settings_group(modules, "AppId", app).await?;
        Ok(AliPayWebSettings {
            app_id: setting(&group, "AppId")?,
            app_private_key: setting(&group, "AppPrivateKey")?,
            alipay_public_key: setting(&group, "AliPayPublicKey")?,
            gateway_url: ALIPAY_GATEWAY_URL.into(),
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
        let Some(order_model) = self.payable_order(order_no).await? else {
            return Ok(None);
        };
        let settings = self.ali_settings(&[ALIPAY_WEB_MODULE], None).await?;
        let biz_content = json!({
            "subject": build_order_title(&order_model),
            "total_amount": format!("{:.2}", order_model.price),
            "out_trade_no": order_model.order_no,
            "product_code": "FAST_INSTANT_TRADE_PAY",
            "qr_pay_mode": "4",
        });

        alipay::build_alipay_gateway_url(
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
        let Some(order_model) = self.payable_order(order_no).await? else {
            return Ok(None);
        };
        let settings = self.ali_settings(&[ALIPAY_WEB_MODULE], None).await?;
        let biz_content = json!({
            "subject": build_order_title(&order_model),
            "total_amount": format!("{:.2}", order_model.price),
            "out_trade_no": order_model.order_no,
            "product_code": "QUICK_WAP_WAY",
            "quit_url": quit_url.unwrap_or_default(),
        });

        alipay::build_alipay_gateway_url(
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
        let Some(order_model) = self.payable_order(order_no).await? else {
            return Ok(None);
        };
        let settings = self.ali_settings(&[ALIPAY_MINI_APP_MODULE], None).await?;
        // 买家来自订单创建人的绑定，不从当前 HTTP 用户推断。
        let Some(buyer_id) = self
            .load_alipay_mini_app_buyer_id(order_model.create_user_id, &settings.app_id)
            .await?
        else {
            return Ok(None);
        };

        let biz_content = json!({
            "subject": build_order_title(&order_model),
            "total_amount": format!("{:.2}", order_model.price),
            "out_trade_no": order_model.order_no,
            "buyer_id": buyer_id,
        });

        let response = alipay::execute_alipay_api(
            self.context.http_client_factory.client(),
            &settings,
            "alipay.trade.create",
            biz_content,
            Some(notify_url),
            "utf-8",
        )
        .await?;

        Ok(Some(response))
    }

    pub async fn process_alipay_notify(
        &self,
        parameters: HashMap<String, String>,
    ) -> Result<bool, AppError> {
        let result = async {
            let field = |key: &str| {
                parameters
                    .get(key)
                    .map(String::as_str)
                    .filter(|v| !v.trim().is_empty())
                    .ok_or_else(|| AppError::bad_request(format!("支付宝通知缺少 {key}")))
            };
            let settings = self
                .ali_settings(
                    &[ALIPAY_WEB_MODULE, ALIPAY_MINI_APP_MODULE],
                    Some(field("app_id")?),
                )
                .await?;
            let notice = alipay::read_notification(&parameters, &settings)?;
            self.apply_payment(notice, "支付宝").await
        }
        .await;
        Ok(notification_result(result))
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
        let rows = user_bind_external::Entity::find_active()
            .filter(user_bind_external::Column::UserId.eq(user_id))
            .filter(user_bind_external::Column::AppName.eq(ALIPAY_MINI_APP_MODULE))
            .filter(user_bind_external::Column::AppId.eq(app_id))
            .limit(2)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;
        Ok(unique(rows)?
            .map(|item| item.open_id)
            .filter(|v| !v.trim().is_empty()))
    }
}

/// 构建订单标题
fn build_order_title(order_model: &order::Model) -> String {
    format!("{}交易", order_model.create_time.format("%Y%m%d%H%M"))
}

/// 转换数据库异常
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}

// ===== 微信支付 =====

/// 微信支付模块名
const WX_PAY_MODULE: &str = "WeiXinPay";

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
        if open_id.trim().is_empty() {
            return Err(AppError::bad_request("支付场景参数不能为空"));
        }
        let Some(order_model) = self.payable_order(order_no).await? else {
            return Ok(None);
        };
        let settings = self.wx_settings(None).await?;
        let key = format!(
            "Payment:WeiXin:JSAPI:{}:{}:{}:{}",
            settings.mch_id, settings.app_id, order_model.id, order_model.price
        );
        let key = format!(
            "{key}:{}",
            common::crypto_helper::md5_hash_data(
                open_id,
                common::crypto_helper::StringEncoding::Hex
            )
            .to_ascii_uppercase()
        );
        self.get_or_create_wx_cached_result(&key, || async move {
            let order = prepay_order(&order_model, notify_url, &settings);
            self.wx_client()
                .jsapi(&settings, &order, open_id)
                .await
                .map(Some)
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
        let Some(order_model) = self.payable_order(order_no).await? else {
            return Ok(None);
        };
        let settings = self.wx_settings(None).await?;
        let key = format!(
            "Payment:WeiXin:App:{}:{}:{}:{}",
            settings.mch_id, settings.app_id, order_model.id, order_model.price
        );
        self.get_or_create_wx_cached_result(&key, || async move {
            let order = prepay_order(&order_model, notify_url, &settings);
            self.wx_client().app(&settings, &order).await.map(Some)
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
        if client_ip.trim().is_empty() {
            return Err(AppError::bad_request("支付场景参数不能为空"));
        }
        let Some(order_model) = self.payable_order(order_no).await? else {
            return Ok(None);
        };
        let settings = self.wx_settings(None).await?;
        let key = format!(
            "Payment:WeiXin:H5:{}:{}:{}:{}",
            settings.mch_id, settings.app_id, order_model.id, order_model.price
        );
        let key = format!(
            "{key}:{}",
            common::crypto_helper::md5_hash_data(
                client_ip,
                common::crypto_helper::StringEncoding::Hex
            )
            .to_ascii_uppercase()
        );
        self.get_or_create_wx_cached_link(&key, Duration::from_secs(270), || async move {
            let order = prepay_order(&order_model, notify_url, &settings);
            self.wx_client()
                .h5(&settings, &order, client_ip)
                .await
                .map(Some)
        })
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
        let Some(order_model) = self.payable_order(order_no).await? else {
            return Ok(None);
        };
        let settings = self.wx_settings(None).await?;
        let key = format!(
            "Payment:WeiXin:PC:{}:{}:{}:{}",
            settings.mch_id, settings.app_id, order_model.id, order_model.price
        );
        self.get_or_create_wx_cached_link(&key, Duration::from_secs(900), || async move {
            let order = prepay_order(&order_model, notify_url, &settings);
            self.wx_client().pc(&settings, &order).await.map(Some)
        })
        .await
    }

    pub async fn process_wx_pay_notify(
        &self,
        mch_id: &str,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<bool, AppError> {
        let result = async {
            if mch_id.trim().is_empty() {
                return Ok(false);
            }
            let settings = self.wx_settings(Some(mch_id)).await?;
            let notice = self
                .wx_client()
                .read_notification(&settings, headers, body)
                .await?;
            self.apply_payment(notice, "微信支付").await
        }
        .await;
        Ok(notification_result(result))
    }

    async fn apply_payment(
        &self,
        notice: PaymentNotification,
        channel: &str,
    ) -> Result<bool, AppError> {
        let PaymentNotification {
            order_no,
            transaction_id,
            amount,
            paid_at,
        } = notice;
        let transaction = transaction_id.as_str();
        let guard = self
            .context
            .distributed_lock
            .lock(&format!("Payment:Order:{order_no}"), Duration::ZERO, 1)
            .await?;
        let result = async {
            let Some(order) = self.load_payment_order(&order_no).await? else {
                return Ok(false);
            };
            let same = |o: &order::Model| {
                o.pay_state
                    && o.price == amount
                    && o.pay_price == Some(amount)
                    && o.pay_type == channel
                    && o.serial_no.as_deref() == Some(transaction)
            };
            if order.pay_state {
                return Ok(same(&order));
            }
            if order.price != amount || order.state != "待支付" {
                return Ok(false);
            }
            let id = order.id;
            let mut active = order.into_active_model();
            active.pay_price = Set(Some(amount));
            active.serial_no = Set(Some(transaction.into()));
            active.pay_state = Set(true);
            active.pay_time = Set(Some(paid_at.into()));
            active.pay_type = Set(channel.into());
            active.state = Set("已支付".into());
            match active.update_versioned(self.context.db).await {
                Ok(_) => Ok(true),
                Err(sea_orm::DbErr::RecordNotUpdated) => Ok(order::Entity::find_active()
                    .filter(order::Column::Id.eq(id))
                    .one(self.context.db)
                    .await
                    .map_err(to_app_error)?
                    .as_ref()
                    .is_some_and(same)),
                Err(e) => Err(to_app_error(e)),
            }
        }
        .await;
        guard.release().await;
        result
    }
}

fn unique<T>(mut rows: Vec<T>) -> Result<Option<T>, AppError> {
    if rows.len() > 1 {
        return Err(AppError::bad_request("支付记录不唯一"));
    }
    Ok(rows.pop())
}

fn setting(group: &[app_setting::Model], key: &str) -> Result<String, AppError> {
    let mut rows = group.iter().filter(|r| r.key == key);
    rows.next()
        .filter(|r| rows.next().is_none() && !r.value.trim().is_empty())
        .map(|r| r.value.clone())
        .ok_or_else(|| AppError::bad_request(format!("支付配置项 {key} 缺失或重复")))
}

fn notification_result(result: Result<bool, AppError>) -> bool {
    result.unwrap_or_else(|error| {
        tracing::error!(reason = %error.message(),"支付通知处理失败");
        false
    })
}

pub(super) struct PaymentNotification {
    pub order_no: String,
    pub transaction_id: String,
    pub amount: Decimal,
    pub paid_at: chrono::DateTime<Utc>,
}

fn prepay_order<'a>(
    order: &'a order::Model,
    notify_url: &str,
    settings: &WxPaySettings,
) -> PrepayOrder<'a> {
    PrepayOrder {
        number: &order.order_no,
        title: build_order_title(order),
        amount: order.price,
        notify_url: format!("{notify_url}{}", settings.mch_id),
    }
}
