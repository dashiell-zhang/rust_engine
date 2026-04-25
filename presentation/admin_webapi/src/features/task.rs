use crate::{
    features::protect_method_router,
    handlers::task::{
        create_schedule_task, get_args_schedule_task_name_list, get_queue_task_list,
        get_task_setting_list, retry_queue_task, update_task_setting,
    },
    openapi::task::TaskApiDoc,
    state::AppState,
};
use axum::Router;
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};

use super::{WebApiFeature, WebApiRoutePermission};

/// 任务中心特性
pub struct TaskFeature;

impl WebApiFeature for TaskFeature {
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/Operations/GetTaskSettingList",
                protect_method_router(
                    axum::routing::get(get_task_setting_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/GetTaskSettingList",
                    false,
                ),
            )
            .route(
                "/Operations/UpdateTaskSetting",
                protect_method_router(
                    axum::routing::post(update_task_setting),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/UpdateTaskSetting",
                    false,
                ),
            )
            .route(
                "/Operations/GetArgsScheduleTaskNameList",
                protect_method_router(
                    axum::routing::get(get_args_schedule_task_name_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/GetArgsScheduleTaskNameList",
                    false,
                ),
            )
            .route(
                "/Operations/CreateScheduleTask",
                protect_method_router(
                    axum::routing::post(create_schedule_task),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/CreateScheduleTask",
                    false,
                ),
            )
            .route(
                "/Operations/GetQueueTaskList",
                protect_method_router(
                    axum::routing::get(get_queue_task_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Operations/GetQueueTaskList",
                    false,
                ),
            )
            .route(
                "/Operations/RetryQueueTask",
                protect_method_router(
                    axum::routing::post(retry_queue_task),
                    app_state,
                    permission_module,
                    "/Operations/RetryQueueTask",
                    false,
                ),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(TaskApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![
            WebApiRoutePermission {
                route: "/Operations/GetTaskSettingList",
                remark: "获取任务配置列表",
            },
            WebApiRoutePermission {
                route: "/Operations/UpdateTaskSetting",
                remark: "更新任务配置",
            },
            WebApiRoutePermission {
                route: "/Operations/GetArgsScheduleTaskNameList",
                remark: "获取支持参数的定时任务名称列表",
            },
            WebApiRoutePermission {
                route: "/Operations/CreateScheduleTask",
                remark: "创建带参定时任务",
            },
            WebApiRoutePermission {
                route: "/Operations/GetQueueTaskList",
                remark: "获取队列任务列表",
            },
            WebApiRoutePermission {
                route: "/Operations/RetryQueueTask",
                remark: "重试队列任务",
            },
        ]
    }
}
