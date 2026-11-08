use application_model::message::models::SendEmailDto;
use async_trait::async_trait;
use common::app_error::AppError;

/// 邮件消息发送用例；文件和传输适配器由宿主装配。
#[async_trait]
pub trait MessageService: Send + Sync {
    async fn send_email(&self, email: SendEmailDto) -> Result<(), AppError>;
}
