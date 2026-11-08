use crate::openapi::task::TaskApiDoc;
use utoipa::OpenApi;
use utoipa::openapi::OpenApi as OpenApiDocument;
use webapi_core::host::features::WebApiFeature;

/// 任务中心特性
pub struct TaskFeature;

impl WebApiFeature for TaskFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::TaskCenter>(resources)?;
        Ok(())
    }

    fn endpoints(&self) -> Vec<webapi_core::endpoint::Endpoint> {
        vec![
            webapi_core::endpoint!(crate::handlers::task::get_task_setting_list),
            webapi_core::endpoint!(crate::handlers::task::update_task_setting),
            webapi_core::endpoint!(crate::handlers::task::get_args_schedule_task_name_list),
            webapi_core::endpoint!(crate::handlers::task::create_schedule_task),
            webapi_core::endpoint!(crate::handlers::task::get_queue_task_list),
            webapi_core::endpoint!(crate::handlers::task::retry_queue_task),
        ]
    }

    /// 补充类型与标签，端点操作由 endpoints 自动生成。
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(TaskApiDoc::openapi())
    }
}
