use crate::state::AppState;
use webapi_support::WebApiApplicationExt;

/// 客户端 LLM 应用服务适配器
pub struct ClientLlmApplicationAdapter<'a> {
    /// WebAPI 宿主状态
    state: &'a AppState,
}

impl<'a> ClientLlmApplicationAdapter<'a> {
    /// 创建客户端 LLM 应用服务适配器
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// 创建 LLM 运行时调用服务
    pub fn llm_invoke(
        &self,
    ) -> Box<dyn application_interface::services::llm_invoke::LlmInvokeService + '_> {
        self.state.services().llm_invoke()
    }
}
