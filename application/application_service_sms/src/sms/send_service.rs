use application_model::message::models::SendSmsDto;
use application_runtime::context::SmsContext;
use async_trait::async_trait;
use common::app_error::AppError;
use sms::SendSmsRequest;

/// 短信发送应用服务
pub struct SendSmsService<'a> {
    /// 短信上下文
    context: SmsContext<'a>,
}

impl<'a> SendSmsService<'a> {
    /// 创建短信发送应用服务
    pub fn new(context: SmsContext<'a>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl application_interface::services::sms::SmsService for SendSmsService<'_> {
    /// 源应用服务返回 Task，等待提供商完成但不使用其 bool 结果。
    async fn send_sms(&self, message: SendSmsDto) -> Result<(), AppError> {
        self.context
            .sms_sender
            .send_sms(SendSmsRequest {
                phone: message.phone,
                sign_name: message.sign_name,
                template_code: message.template_code,
                template_params: message.template_params,
            })
            .await?;
        Ok(())
    }
}
