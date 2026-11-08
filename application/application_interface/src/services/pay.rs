use application_model::pay::models::{
    WxPayAppDto, WxPayJsapiDto, WxPayNotifyDto, WxPayNotifyRetDto,
};
use async_trait::async_trait;
use common::app_error::AppError;
use std::collections::HashMap;

/// 支付请求与回调应用契约。
#[async_trait]
pub trait PayService: Send + Sync {
    async fn create_alipay_pc(
        &self,
        order_no: &str,
        notify_url: &str,
        return_url: Option<&str>,
    ) -> Result<Option<String>, AppError>;
    async fn create_alipay_h5(
        &self,
        order_no: &str,
        notify_url: &str,
        return_url: Option<&str>,
        quit_url: Option<&str>,
    ) -> Result<Option<String>, AppError>;
    async fn create_alipay_mini_app(
        &self,
        order_no: &str,
        notify_url: &str,
    ) -> Result<Option<String>, AppError>;
    async fn process_alipay_notify(
        &self,
        parameters: HashMap<String, String>,
    ) -> Result<Option<String>, AppError>;
    async fn create_wx_pay_jsapi(
        &self,
        order_no: &str,
        open_id: &str,
        notify_url: &str,
    ) -> Result<Option<WxPayJsapiDto>, AppError>;
    async fn create_wx_pay_app(
        &self,
        order_no: &str,
        notify_url: &str,
    ) -> Result<Option<WxPayAppDto>, AppError>;
    async fn create_wx_pay_h5(
        &self,
        order_no: &str,
        notify_url: &str,
        client_ip: &str,
    ) -> Result<Option<String>, AppError>;
    async fn create_wx_pay_pc(
        &self,
        order_no: &str,
        notify_url: &str,
    ) -> Result<Option<String>, AppError>;
    async fn process_wx_pay_notify(
        &self,
        mch_id: &str,
        notify_dto: WxPayNotifyDto,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<WxPayNotifyRetDto, AppError>;
    /// 微信退款模板入口。
    async fn wx_pay_refund(&self) -> Result<(), AppError>;
    /// 微信退款查询模板入口。
    async fn wx_pay_refund_query(&self) -> Result<(), AppError>;
    /// 支付宝退款模板入口。
    async fn alipay_refund(&self) -> Result<(), AppError>;
    /// 支付宝退款查询模板入口。
    async fn alipay_refund_query(&self) -> Result<(), AppError>;
}
