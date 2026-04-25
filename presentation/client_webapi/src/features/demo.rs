use axum::Router;
use std::sync::Arc;
use utoipa::openapi::OpenApi as OpenApiDocument;

use crate::{
    handlers::demo::{
        a_ref, add, add2, echo, echo2, stream_numbers, stream_numbers2, test_llm, test_llm_stream,
    },
    state::AppState,
};

use super::WebApiFeature;

/// 仅在调试构建中开放的客户端演示特性
/// 开发调试用 Demo 特性
pub struct DemoFeature;

impl WebApiFeature for DemoFeature {
    /// 注册 Demo 路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        _app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        // 这些路由用于联调验证 不进入正式权限体系
        router
            .route("/Demo/Echo", axum::routing::get(echo))
            .route("/Demo/Add", axum::routing::get(add))
            .route("/Demo/Echo2", axum::routing::get(echo2))
            .route("/Demo/Add2", axum::routing::post(add2))
            .route("/Demo/ARef", axum::routing::get(a_ref))
            .route("/Demo/StreamNumbers", axum::routing::get(stream_numbers))
            .route("/Demo/StreamNumbers2", axum::routing::get(stream_numbers2))
            .route("/Demo/TestLLM", axum::routing::get(test_llm))
            .route("/Demo/TestLLMStream", axum::routing::get(test_llm_stream))
    }

    /// 当前特性暂不提供独立 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        None
    }
}
