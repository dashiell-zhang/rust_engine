use async_trait::async_trait;
use common::app_error::AppError;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// 短信发送请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendSmsRequest {
    /// 接收手机号
    pub phone: Option<String>,

    /// 短信签名
    pub sign_name: Option<String>,

    /// 模板编号
    pub template_code: Option<String>,

    /// 模板参数
    pub template_params: Option<IndexMap<String, Option<String>>>,
}

/// 通用短信发送契约
#[async_trait]
pub trait AppSmsSender: Send + Sync {
    /// 发送短信；内置提供商将发送异常转换为 false，应用层不据此触发重试。
    async fn send_sms(&self, request: SendSmsRequest) -> Result<bool, AppError>;
}
