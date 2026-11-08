use application_model::pay::models::{
    WxPayAppDto, WxPayJsapiDto, WxPayNotifyDto, WxPayNotifyRetDto,
};
use async_trait::async_trait;
use common::app_error::AppError;
use std::collections::HashMap;

#[async_trait]
impl application_interface::services::pay::PayService for crate::pay::service::PayService<'_> {
    async fn create_alipay_pc(
        &self,
        order_no: &str,
        notify_url: &str,
        return_url: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        crate::pay::service::PayService::create_alipay_pc(self, order_no, notify_url, return_url)
            .await
    }
    async fn create_alipay_h5(
        &self,
        order_no: &str,
        notify_url: &str,
        return_url: Option<&str>,
        quit_url: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        crate::pay::service::PayService::create_alipay_h5(
            self, order_no, notify_url, return_url, quit_url,
        )
        .await
    }
    async fn create_alipay_mini_app(
        &self,
        order_no: &str,
        notify_url: &str,
    ) -> Result<Option<String>, AppError> {
        crate::pay::service::PayService::create_alipay_mini_app(self, order_no, notify_url).await
    }
    async fn process_alipay_notify(
        &self,
        parameters: HashMap<String, String>,
    ) -> Result<Option<String>, AppError> {
        crate::pay::service::PayService::process_alipay_notify(self, parameters).await
    }
    async fn create_wx_pay_jsapi(
        &self,
        order_no: &str,
        open_id: &str,
        notify_url: &str,
    ) -> Result<Option<WxPayJsapiDto>, AppError> {
        crate::pay::service::PayService::create_wx_pay_jsapi(self, order_no, open_id, notify_url)
            .await
    }
    async fn create_wx_pay_app(
        &self,
        order_no: &str,
        notify_url: &str,
    ) -> Result<Option<WxPayAppDto>, AppError> {
        crate::pay::service::PayService::create_wx_pay_app(self, order_no, notify_url).await
    }
    async fn create_wx_pay_h5(
        &self,
        order_no: &str,
        notify_url: &str,
        client_ip: &str,
    ) -> Result<Option<String>, AppError> {
        crate::pay::service::PayService::create_wx_pay_h5(self, order_no, notify_url, client_ip)
            .await
    }
    async fn create_wx_pay_pc(
        &self,
        order_no: &str,
        notify_url: &str,
    ) -> Result<Option<String>, AppError> {
        crate::pay::service::PayService::create_wx_pay_pc(self, order_no, notify_url).await
    }
    async fn process_wx_pay_notify(
        &self,
        mch_id: &str,
        notify_dto: WxPayNotifyDto,
        headers: &HashMap<String, String>,
        body: &str,
    ) -> Result<WxPayNotifyRetDto, AppError> {
        crate::pay::service::PayService::process_wx_pay_notify(
            self, mch_id, notify_dto, headers, body,
        )
        .await
    }
    /// 微信退款模板入口。
    async fn wx_pay_refund(&self) -> Result<(), AppError> {
        crate::pay::service::PayService::wx_pay_refund(self).await
    }
    /// 微信退款查询模板入口。
    async fn wx_pay_refund_query(&self) -> Result<(), AppError> {
        crate::pay::service::PayService::wx_pay_refund_query(self).await
    }
    /// 支付宝退款模板入口。
    async fn alipay_refund(&self) -> Result<(), AppError> {
        crate::pay::service::PayService::alipay_refund(self).await
    }
    /// 支付宝退款查询模板入口。
    async fn alipay_refund_query(&self) -> Result<(), AppError> {
        crate::pay::service::PayService::alipay_refund_query(self).await
    }
}
