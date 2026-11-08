use application_interface::services::TaskServiceFactory;
use application_runtime::context::{AppContext, MessageContext, SmsContext};
use std::sync::Arc;
use task_service_core::features::TaskServiceFeatureContext;

/// 此处是任务宿主选择具体应用服务的唯一装配点。
pub fn business_services(context: &TaskServiceFeatureContext) -> Arc<dyn TaskServiceFactory> {
    let database = context
        .modules
        .get::<repository::BusinessModule>()
        .expect("内置任务必须注册 BusinessModule")
        .clone();
    Arc::new(BusinessTaskServices {
        context: context.clone(),
        database,
    })
}
struct BusinessTaskServices {
    context: TaskServiceFeatureContext,
    database: repository_core::module::ModuleDatabase<repository::BusinessModule>,
}
impl TaskServiceFactory for BusinessTaskServices {
    fn message(&self) -> Box<dyn application_interface::services::message::MessageService + '_> {
        Box::new(application_service::message::service::MessageService::new(
            MessageContext {
                app: AppContext {
                    db: self.database.write(),
                    id_generator: &self.context.identifier_generator,
                    current_user: None,
                },
                root_path: self.context.content_root.as_path(),
                file_storage: self.context.file_storage.as_deref(),
                email_sender: Some(self.context.email_sender.as_ref()),
            },
        ))
    }
    fn sms(&self) -> Box<dyn application_interface::services::sms::SmsService + '_> {
        Box::new(
            application_service_sms::sms::send_service::SendSmsService::new(SmsContext {
                sms_sender: self.context.sms_sender.as_ref(),
            }),
        )
    }
}
