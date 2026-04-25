use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 邮件附件消息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SendEmailAttachmentMessage {
    /// 附件文件名
    pub name: String,

    /// 附件内容字节
    pub content: Vec<u8>,
}

/// 邮件发送参数
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SendEmailDto {
    /// 发件人显示名称
    pub from_display_name: Option<String>,

    /// 收件人地址列表
    pub to_addresses: Vec<String>,

    /// 邮件标题
    pub subject: String,

    /// 邮件正文
    pub body: String,

    /// 正文是否为 HTML 格式
    pub is_body_html: bool,

    /// 附件文件 ID 列表（对应 StoredFile 表）
    pub file_id_list: Option<Vec<i64>>,
}

/// 邮件发送任务参数
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SendEmailMessage {
    /// 发件人显示名称
    pub from_display_name: Option<String>,

    /// 收件人地址列表
    pub to_addresses: Vec<String>,

    /// 邮件标题
    pub subject: String,

    /// 邮件正文
    pub body: String,

    /// 正文是否为 HTML 格式
    pub is_body_html: bool,

    /// 附件列表
    pub attachments: Vec<SendEmailAttachmentMessage>,
}

/// 短信发送任务参数
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SendSmsMessage {
    /// 接收手机号
    pub phone: String,

    /// 短信签名
    pub sign_name: String,

    /// 模板编号
    pub template_code: String,

    /// 模板参数
    #[schema(value_type = Object)]
    pub template_params: IndexMap<String, String>,
}
