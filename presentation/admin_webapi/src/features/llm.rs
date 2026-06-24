use crate::{
    features::protect_method_router,
    handlers::llm::{
        create_llm_app, create_llm_model, delete_llm_app, delete_llm_model, get_llm_app_list,
        get_llm_conversation_list, get_llm_model_list, get_llm_model_select, test_llm_app,
        update_llm_app, update_llm_model,
    },
    openapi::llm::LlmApiDoc,
    state::AppState,
};
use axum::Router;
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};

use super::{WebApiFeature, WebApiRoutePermission};

/// 管理端 LLM 能力特性
pub struct LlmFeature;

impl WebApiFeature for LlmFeature {
    /// 注册 LLM 管理相关路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/LlmModel/GetLlmModelList",
                protect_method_router(
                    axum::routing::get(get_llm_model_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/LlmModel/GetLlmModelList",
                    false,
                ),
            )
            .route(
                "/LlmModel/GetLlmModelSelect",
                protect_method_router(
                    axum::routing::get(get_llm_model_select),
                    app_state.clone(),
                    permission_module.clone(),
                    "/LlmModel/GetLlmModelSelect",
                    false,
                ),
            )
            .route(
                "/LlmModel/CreateLlmModel",
                protect_method_router(
                    axum::routing::post(create_llm_model),
                    app_state.clone(),
                    permission_module.clone(),
                    "/LlmModel/CreateLlmModel",
                    false,
                ),
            )
            .route(
                "/LlmModel/UpdateLlmModel",
                protect_method_router(
                    axum::routing::post(update_llm_model),
                    app_state.clone(),
                    permission_module.clone(),
                    "/LlmModel/UpdateLlmModel",
                    false,
                ),
            )
            .route(
                "/LlmModel/DeleteLlmModel",
                protect_method_router(
                    axum::routing::delete(delete_llm_model),
                    app_state.clone(),
                    permission_module.clone(),
                    "/LlmModel/DeleteLlmModel",
                    false,
                ),
            )
            .route(
                "/Operations/GetLlmAppList",
                protect_method_router(
                    axum::routing::get(get_llm_app_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/GetLlmAppList",
                    false,
                ),
            )
            .route(
                "/Operations/CreateLlmApp",
                protect_method_router(
                    axum::routing::post(create_llm_app),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/CreateLlmApp",
                    false,
                ),
            )
            .route(
                "/Operations/UpdateLlmApp",
                protect_method_router(
                    axum::routing::post(update_llm_app),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/UpdateLlmApp",
                    false,
                ),
            )
            .route(
                "/Operations/DeleteLlmApp",
                protect_method_router(
                    axum::routing::delete(delete_llm_app),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/DeleteLlmApp",
                    false,
                ),
            )
            .route(
                "/Operations/TestLlmApp",
                protect_method_router(
                    axum::routing::post(test_llm_app),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/TestLlmApp",
                    false,
                ),
            )
            .route(
                "/Operations/GetLlmConversationList",
                protect_method_router(
                    axum::routing::get(get_llm_conversation_list),
                    app_state,
                    permission_module,
                    "/Operations/GetLlmConversationList",
                    false,
                ),
            )
    }

    /// 获取 LLM 模块 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(LlmApiDoc::openapi())
    }

    /// 获取 LLM 模块权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![
            WebApiRoutePermission {
                route: "/LlmModel/GetLlmModelList",
                remark: "获取 LLM 模型配置列表",
            },
            WebApiRoutePermission {
                route: "/LlmModel/GetLlmModelSelect",
                remark: "获取 LLM 模型下拉列表",
            },
            WebApiRoutePermission {
                route: "/LlmModel/CreateLlmModel",
                remark: "创建 LLM 模型配置",
            },
            WebApiRoutePermission {
                route: "/LlmModel/UpdateLlmModel",
                remark: "更新 LLM 模型配置",
            },
            WebApiRoutePermission {
                route: "/LlmModel/DeleteLlmModel",
                remark: "删除 LLM 模型配置",
            },
            WebApiRoutePermission {
                route: "/Operations/GetLlmAppList",
                remark: "获取 LLM 应用配置列表",
            },
            WebApiRoutePermission {
                route: "/Operations/CreateLlmApp",
                remark: "创建 LLM 应用配置",
            },
            WebApiRoutePermission {
                route: "/Operations/UpdateLlmApp",
                remark: "更新 LLM 应用配置",
            },
            WebApiRoutePermission {
                route: "/Operations/DeleteLlmApp",
                remark: "删除 LLM 应用配置",
            },
            WebApiRoutePermission {
                route: "/Operations/TestLlmApp",
                remark: "测试 LLM 应用配置",
            },
            WebApiRoutePermission {
                route: "/Operations/GetLlmConversationList",
                remark: "获取 LLM 对话记录列表",
            },
        ]
    }
}
