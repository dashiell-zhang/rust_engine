use application_model::message::models::SendEmailDto;
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::message::MessageService
    for crate::message::service::MessageService<'_>
{
    async fn send_email(&self, email: SendEmailDto) -> Result<(), AppError> {
        crate::message::service::MessageService::send_email(self, email).await
    }
}
