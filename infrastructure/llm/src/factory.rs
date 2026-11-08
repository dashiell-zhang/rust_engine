//! 通用 LLM 装配契约；模型配置的数据库读取由应用层实现。
use crate::{CancellationToken, LlmClient, LlmError, LlmModelConfig};
use async_trait::async_trait;
use common::app_error::AppError;
use http_client::HttpClientFactory;
use std::sync::Arc;

pub trait LlmClientFactory: Send + Sync {
    fn create_client(&self, config: LlmModelConfig) -> Result<Arc<dyn LlmClient>, LlmError>;
}

/// 默认实现只保存宿主共享工厂，每次调用重新取得轮换后的客户端。
pub struct DynamicLlmClientFactory {
    http_clients: Arc<HttpClientFactory>,
}
impl DynamicLlmClientFactory {
    pub fn new(http_clients: Arc<HttpClientFactory>) -> Self {
        Self { http_clients }
    }
}
impl LlmClientFactory for DynamicLlmClientFactory {
    fn create_client(&self, config: LlmModelConfig) -> Result<Arc<dyn LlmClient>, LlmError> {
        crate::create_client(self.http_clients.streaming_client(), config)
    }
}

#[async_trait]
pub trait LlmModelConfigResolver: Send + Sync {
    /// 返回启用模型的运行配置；缺失与查询失败分开表达。
    /// 查询应在返回的 future 内执行，释放 future 必须取消等待，不能产生脱离调用方的后台任务。
    async fn get_config(&self, model_id: i64) -> Result<Option<LlmModelConfig>, AppError>;

    async fn get_config_with_cancellation(
        &self,
        model_id: i64,
        token: CancellationToken,
    ) -> Result<Option<LlmModelConfig>, AppError> {
        common::cancellation::run(&token, self.get_config(model_id)).await
    }
}
