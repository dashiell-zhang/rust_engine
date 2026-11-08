use async_trait::async_trait;
use common::app_error::AppError;
use email::{AppEmailSender, SendEmailAttachment, SendEmailRequest};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Attachment, Mailbox, MultiPart, SinglePart, header::ContentType},
    transport::smtp::authentication::Credentials,
};
use serde::Deserialize;

/// SMTP 邮件发送器配置
#[derive(Debug, Clone, Deserialize)]
pub struct SmtpEmailSettings {
    /// SMTP 服务器地址
    #[serde(default)]
    pub host: String,

    /// SMTP 端口
    #[serde(default = "default_smtp_port")]
    pub port: u16,

    /// SMTP 登录用户名
    /// 通常为发件邮箱
    #[serde(default)]
    pub username: String,

    /// SMTP 登录密码或授权码
    #[serde(default)]
    pub password: String,
}

impl Default for SmtpEmailSettings {
    /// 创建默认 SMTP 邮件发送配置
    fn default() -> Self {
        Self {
            host: String::new(),
            port: default_smtp_port(),
            username: String::new(),
            password: String::new(),
        }
    }
}

/// SMTP 邮件发送器
pub struct SmtpEmailSender {
    /// SMTP 配置
    settings: SmtpEmailSettings,
}

impl SmtpEmailSender {
    /// 创建 SMTP 邮件发送器
    /// 使用宿主传入的 SMTP 配置初始化实例
    pub fn new(settings: SmtpEmailSettings) -> Self {
        Self { settings }
    }

    /// 校验 SMTP 配置
    /// 在真正发信前提前拦截空配置
    fn validate_settings(&self) -> Result<(), AppError> {
        if self.settings.host.trim().is_empty() {
            return Err(AppError::internal(
                "SMTP host 未配置，请检查 [message.smtp].host",
            ));
        }

        if self.settings.username.trim().is_empty() {
            return Err(AppError::internal(
                "SMTP username 未配置，请检查 [message.smtp].username",
            ));
        }

        if self.settings.password.trim().is_empty() {
            return Err(AppError::internal(
                "SMTP password 未配置，请检查 [message.smtp].password",
            ));
        }

        Ok(())
    }

    /// 构造邮件消息体
    /// 负责正文 附件 收件人和发件人头部组装
    fn build_message(&self, request: SendEmailRequest) -> Result<Message, AppError> {
        let body_part = if request.is_body_html {
            SinglePart::builder()
                .header(ContentType::TEXT_HTML)
                .body(request.body.clone())
        } else {
            SinglePart::builder()
                .header(ContentType::TEXT_PLAIN)
                .body(request.body.clone())
        };

        let has_attachments = !request.attachments.is_empty();
        let mut attachment_parts = Vec::with_capacity(request.attachments.len());
        for attachment in request.attachments {
            let (name, bytes) = match attachment {
                SendEmailAttachment::LocalFile { name, path } => {
                    let bytes = std::fs::read(&path).map_err(|error| {
                        AppError::internal(format!("读取附件文件失败 [{path}]: {error}"))
                    })?;
                    (name, bytes)
                }
                SendEmailAttachment::Memory { name, content } => (name, content),
            };

            let content_type = ContentType::parse("application/octet-stream")
                .map_err(|error| AppError::internal(format!("邮件附件类型无效: {error}")))?;
            attachment_parts.push(Attachment::new(name).body(bytes, content_type));
        }

        let from_address = self
            .settings
            .username
            .parse()
            .map_err(|_| AppError::internal("发件人地址格式无效"))?;
        let from_mailbox = Mailbox::new(request.from_display_name, from_address);

        let mut message_builder = Message::builder().from(from_mailbox);
        for to_addr in &request.to_addresses {
            let mailbox = to_addr
                .parse()
                .map_err(|_| AppError::internal("收件人地址格式无效"))?;
            message_builder = message_builder.to(mailbox);
        }

        let message_builder = message_builder.subject(request.subject);
        let message = if has_attachments {
            let mut multipart = MultiPart::mixed().singlepart(body_part);
            for attachment in attachment_parts {
                multipart = multipart.singlepart(attachment);
            }
            message_builder.multipart(multipart)
        } else {
            message_builder.singlepart(body_part)
        };
        message.map_err(|error| AppError::internal(format!("邮件构造失败: {error}")))
    }
}

#[async_trait]
impl AppEmailSender for SmtpEmailSender {
    /// 发送邮件
    /// 使用 SMTP 连接完成实际投递
    async fn send_email(&self, request: SendEmailRequest) -> Result<(), AppError> {
        self.validate_settings()?;

        let email = self.build_message(request)?;
        let credentials = Credentials::new(
            self.settings.username.clone(),
            self.settings.password.clone(),
        );
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.settings.host)
            .map_err(|error| AppError::internal(format!("SMTP 连接配置失败: {error}")))?
            .port(self.settings.port)
            .credentials(credentials)
            .build();

        mailer
            .send(email)
            .await
            .map_err(|error| AppError::internal(format!("邮件发送失败: {error}")))?;

        tracing::info!(
            target: "email.smtp.send",
            host = %self.settings.host,
            port = self.settings.port,
            "SMTP 邮件发送成功"
        );

        Ok(())
    }
}

/// 返回默认 SMTP 端口
/// 默认使用 STARTTLS 常见端口
fn default_smtp_port() -> u16 {
    587
}
