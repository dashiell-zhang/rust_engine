#[cfg(feature = "debug-demo")]
use crate::state::AppState;
#[cfg(feature = "debug-demo")]
use application_interface::context::{AppContext, LlmContext};
#[cfg(feature = "debug-demo")]
use application_service_llm::llm::invoke_service::LlmInvokeService;

/// 客户端 LLM 应用服务适配器
#[cfg(feature = "debug-demo")]
pub struct ClientLlmApplicationAdapter<'a> {
    /// WebAPI 宿主状态
    state: &'a AppState,
}

#[cfg(feature = "debug-demo")]
impl<'a> ClientLlmApplicationAdapter<'a> {
    /// 创建客户端 LLM 应用服务适配器
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// 获取 LLM 模块通用上下文
    fn llm_context(&self) -> LlmContext<'_> {
        LlmContext {
            app: self.app_context(),
            distributed_lock: self.state.role_code_lock.as_ref(),
            http_client_factory: &self.state.http_client_factory,
        }
    }

    /// 获取应用层通用上下文
    fn app_context(&self) -> AppContext<'_> {
        AppContext {
            db: self.state.db_pool.as_ref(),
            id_generator: &self.state.id_generator,
            current_user: None,
        }
    }

    /// 创建 LLM 运行时调用服务
    pub fn llm_invoke(&self) -> LlmInvokeService<'_> {
        LlmInvokeService::new(self.llm_context())
    }
}
