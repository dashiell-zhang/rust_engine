use application_interface::{context::SmsContext, sms::SendSmsRequest};
use application_model::message::models::SendSmsMessage;
use common::app_error::AppError;

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

    /// 发送短信消息
    ///
    /// 校验消息内容后转交底层短信发送器执行实际发送
    pub async fn send_message(&self, message: SendSmsMessage) -> Result<(), AppError> {
        self.validate_message(&message)?;

        self.context
            .sms_sender
            .send_sms(SendSmsRequest {
                phone: message.phone,
                sign_name: message.sign_name,
                template_code: message.template_code,
                template_params: message.template_params,
            })
            .await
    }

    /// 根据队列参数发送短信消息
    ///
    /// 将队列中的 JSON 参数反序列化为短信模型后复用统一发送逻辑
    pub async fn send_queue_message(&self, parameter: &str) -> Result<(), AppError> {
        let message = serde_json::from_str::<SendSmsMessage>(parameter)
            .map_err(|error| AppError::bad_request(format!("短信发送任务参数格式无效: {error}")))?;

        self.send_message(message).await
    }

    /// 校验短信消息内容
    fn validate_message(&self, message: &SendSmsMessage) -> Result<(), AppError> {
        if message.phone.trim().is_empty() {
            return Err(AppError::bad_request("短信接收手机号不可以为空"));
        }

        if message.sign_name.trim().is_empty() {
            return Err(AppError::bad_request("短信签名不可以为空"));
        }

        if message.template_code.trim().is_empty() {
            return Err(AppError::bad_request("短信模板编号不可以为空"));
        }

        Ok(())
    }
}
