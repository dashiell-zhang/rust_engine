use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;

use crate::{
    handlers::demo::{
        a_ref, add, add2, echo, echo2, stream_numbers, stream_numbers2, test_llm, test_llm_stream,
    },
    state::AppState,
};

use super::WebApiFeature;

/// 客户端服务代理与 LLM 能力演示特性
pub struct DemoFeature;

impl WebApiFeature for DemoFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::LlmInvoke>(resources)?;
        Ok(())
    }
    /// 注册 Demo 路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        _app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        // 这些路由用于联调验证 不进入正式权限体系
        router
            .route("/api/Demo/Echo", webapi_core::routes::get(echo))
            .route("/api/Demo/Add", webapi_core::routes::get(add))
            .route("/api/Demo/Echo2", webapi_core::routes::get(echo2))
            .route(
                "/api/Demo/Add2",
                webapi_core::routes::post(add2).json_body("dtoArticle"),
            )
            .route("/api/Demo/ARef", webapi_core::routes::get(a_ref))
            .route(
                "/api/Demo/StreamNumbers",
                webapi_core::routes::get(stream_numbers),
            )
            .route(
                "/api/Demo/StreamNumbers2",
                webapi_core::routes::get(stream_numbers2),
            )
            .route("/api/Demo/TestLLM", webapi_core::routes::get(test_llm))
            .route(
                "/api/Demo/TestLLMStream",
                webapi_core::routes::get(test_llm_stream),
            )
    }

    /// 获取演示接口文档。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(crate::openapi::demo::DemoApiDoc::openapi())
    }
}
