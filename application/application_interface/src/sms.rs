use async_trait::async_trait;
use common::app_error::AppError;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// 短信发送请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendSmsRequest {
    /// 接收手机号
    pub phone: String,

    /// 短信签名
    pub sign_name: String,

    /// 模板编号
    pub template_code: String,

    /// 模板参数
    pub template_params: IndexMap<String, String>,
}

/// 应用层短信发送器
#[async_trait]
pub trait AppSmsSender: Send + Sync {
    /// 发送短信
    async fn send_sms(&self, request: SendSmsRequest) -> Result<(), AppError>;
}
