use async_trait::async_trait;
use common::app_error::AppError;
use serde::{Deserialize, Serialize};

/// 邮件附件请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SendEmailAttachment {
    /// 本地文件附件
    LocalFile {
        /// 附件名称
        name: String,
        /// 本地文件路径
        path: String,
    },

    /// 内存附件
    Memory {
        /// 附件名称
        name: String,
        /// 附件内容字节
        content: Vec<u8>,
    },
}

/// 邮件发送请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendEmailRequest {
    /// 收件人地址列表
    pub to_addresses: Vec<String>,

    /// 邮件主题
    pub subject: String,

    /// 邮件正文
    pub body: String,

    /// 正文是否为 HTML 格式
    pub is_body_html: bool,

    /// 发件人显示名称
    /// 未指定时由基础设施层回退为发件账号
    pub from_display_name: Option<String>,

    /// 附件列表
    pub attachments: Vec<SendEmailAttachment>,
}

/// 应用层邮件发送器
#[async_trait]
pub trait AppEmailSender: Send + Sync {
    /// 发送邮件
    /// 由宿主注入具体基础设施实现
    async fn send_email(&self, request: SendEmailRequest) -> Result<(), AppError>;
}
