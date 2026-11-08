use application_model::message::models::SendSmsDto;
use async_trait::async_trait;
use common::app_error::AppError;

/// 短信应用用例；提供商的布尔结果不改变此无返回值契约。
#[async_trait]
pub trait SmsService: Send + Sync {
    /// 按模板发送短信。
    async fn send_sms(&self, message: SendSmsDto) -> Result<(), AppError>;
}
