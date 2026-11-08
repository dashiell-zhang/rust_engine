use std::sync::Arc;

use application_interface::services::TaskServiceFactory;
use async_trait::async_trait;
use chrono::Local;
use common::identifier_generator::IdentifierGenerator;
use queue_task_database::{NewQueueTask, QueueTaskProducer};
use repository::BusinessModule;
use repository_core::module::ModuleDatabase;
use source_generator_runtime::register_task_registry;
use task_service_core::features::TaskServiceFeatureContext;
use task_service_core::registry::{
    QueueTaskContext, QueueTaskDefinition, QueueTaskHandler, ScheduleTaskContext,
    ScheduleTaskDefinition, ScheduleTaskHandler, TaskRegistry,
};
use task_service_core::task_handler::queue_action;

/// 短信发送队列任务名称
pub const SYSTEM_QUEUE_SEND_SMS: &str = "MessageTask.SendSMS";

/// 邮件发送队列任务名称
pub const SYSTEM_QUEUE_SEND_EMAIL: &str = "MessageTask.SendEmail";

/// NetEngine 示例定时任务名称
const DEMO_SCHEDULE_SHOW_TIME: &str = "ShowTime";

/// NetEngine 示例队列任务名称
const DEMO_QUEUE_SHOW_NAME: &str = "ShowName";

/// NetEngine 示例短信任务名称
const DEMO_QUEUE_SEND_SMS: &str = "SendSMS";

/// NetEngine 示例电话任务名称
const DEMO_QUEUE_CALL_PHONE: &str = "CallPhone";

/// NetEngine 示例成功回调任务名称
const DEMO_QUEUE_SHOW_NAME_SUCCESS: &str = "ShowNameSuccess";

/// 内置任务处理器工厂
#[derive(Clone)]
pub struct BuiltinTaskHandlerFactory {
    /// 数据库连接
    database: ModuleDatabase<BusinessModule>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,

    services: Arc<dyn TaskServiceFactory>,
}

impl BuiltinTaskHandlerFactory {
    /// 根据任务特性上下文创建处理器工厂
    pub fn new(context: &TaskServiceFeatureContext, services: Arc<dyn TaskServiceFactory>) -> Self {
        Self {
            database: context
                .modules
                .get::<BusinessModule>()
                .expect("内置任务必须注册 BusinessModule")
                .clone(),
            identifier_generator: context.identifier_generator.clone(),
            services,
        }
    }

    /// 注册时只捕获共享连接与基础设施；任务对象在每次执行时创建。
    pub fn scoped<F, T>(&self, build: F) -> impl Fn() -> Result<T, String> + Send + Sync + use<F, T>
    where
        F: Fn(&Self) -> T + Send + Sync,
    {
        let resources = self.clone();
        move || Ok(build(&resources))
    }

    /// 创建短信发送队列任务处理器
    pub fn send_sms_queue_task(&self) -> SendSmsQueueTask {
        SendSmsQueueTask::new(self.services.clone())
    }

    /// 创建邮件发送队列任务处理器
    pub fn send_email_queue_task(&self) -> SendEmailQueueTask {
        SendEmailQueueTask::new(self.services.clone())
    }

    /// 创建示例定时任务处理器
    pub fn demo_show_time_schedule_task(&self) -> DemoShowTimeScheduleTask {
        DemoShowTimeScheduleTask::new(self.database.clone(), self.identifier_generator.clone())
    }

    /// 创建示例主队列任务处理器
    pub fn demo_show_name_queue_task(&self) -> impl QueueTaskHandler + use<> {
        let task = Arc::new(DemoShowNameQueueTask::new(
            self.database.clone(),
            self.identifier_generator.clone(),
        ));
        queue_action(move |context| {
            let task = task.clone();
            async move { task.run(context).await }
        })
    }

    /// 创建示例短信任务处理器
    pub fn demo_send_sms_queue_task(&self) -> DemoSendSmsQueueTask {
        DemoSendSmsQueueTask
    }

    /// 创建示例电话任务处理器
    pub fn demo_call_phone_queue_task(&self) -> DemoCallPhoneQueueTask {
        DemoCallPhoneQueueTask
    }

    /// 创建示例回调任务处理器
    pub fn demo_show_name_success_queue_task(&self) -> DemoShowNameSuccessQueueTask {
        DemoShowNameSuccessQueueTask
    }
}

/// 注册任务宿主内置任务
pub fn register_builtin_tasks(
    registry: &mut TaskRegistry,
    context: &TaskServiceFeatureContext,
    services: Arc<dyn TaskServiceFactory>,
) -> Result<(), String> {
    let handlers = BuiltinTaskHandlerFactory::new(context, services);

    register_task_registry! {
        registry = registry;

        queue {
            definition: QueueTaskDefinition::new(SYSTEM_QUEUE_SEND_SMS)
                .with_semaphore(1),
            factory: handlers.scoped(BuiltinTaskHandlerFactory::send_sms_queue_task),
        }

        queue {
            definition: QueueTaskDefinition::new(SYSTEM_QUEUE_SEND_EMAIL)
                .with_semaphore(1),
            factory: handlers.scoped(BuiltinTaskHandlerFactory::send_email_queue_task),
        }

        schedule {
            definition: ScheduleTaskDefinition::new(DEMO_SCHEDULE_SHOW_TIME)
                .with_cron("0/1 * * * * ?"),
            factory: handlers.scoped(BuiltinTaskHandlerFactory::demo_show_time_schedule_task),
        }

        queue {
            definition: QueueTaskDefinition::new(DEMO_QUEUE_SHOW_NAME)
                .with_semaphore(5),
            factory: handlers.scoped(BuiltinTaskHandlerFactory::demo_show_name_queue_task),
        }

        queue {
            definition: QueueTaskDefinition::new(DEMO_QUEUE_SEND_SMS)
                .with_semaphore(5),
            factory: handlers.scoped(BuiltinTaskHandlerFactory::demo_send_sms_queue_task),
        }

        queue {
            definition: QueueTaskDefinition::new(DEMO_QUEUE_CALL_PHONE)
                .with_semaphore(5),
            factory: handlers.scoped(BuiltinTaskHandlerFactory::demo_call_phone_queue_task),
        }

        queue {
            definition: QueueTaskDefinition::new(DEMO_QUEUE_SHOW_NAME_SUCCESS)
                .with_semaphore(5),
            factory: handlers.scoped(BuiltinTaskHandlerFactory::demo_show_name_success_queue_task),
        }

    }
}

/// 邮件发送队列任务处理器
pub struct SendEmailQueueTask {
    services: Arc<dyn TaskServiceFactory>,
}
impl SendEmailQueueTask {
    pub fn new(services: Arc<dyn TaskServiceFactory>) -> Self {
        Self { services }
    }
}

#[async_trait]
impl QueueTaskHandler for SendEmailQueueTask {
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let email = context.deserialize_parameter()?;
        let service = self.services.message();
        service
            .send_email(email)
            .await
            .map_err(|error| error.message().to_string())
    }
}

/// 短信发送队列任务处理器
pub struct SendSmsQueueTask {
    services: Arc<dyn TaskServiceFactory>,
}
impl SendSmsQueueTask {
    pub fn new(services: Arc<dyn TaskServiceFactory>) -> Self {
        Self { services }
    }
}

#[async_trait]
impl QueueTaskHandler for SendSmsQueueTask {
    /// 执行短信发送队列任务
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let message = context
            .deserialize_parameter::<Option<_>>()?
            .ok_or_else(|| "短信发送任务参数为空".to_string())?;
        let service = self.services.sms();
        service
            .send_sms(message)
            .await
            .map_err(|error| error.message().to_string())?;

        tracing::info!(
            target: "task_service.queue.execute",
            task_name = %context.name,
            "短信发送队列任务执行完成"
        );

        Ok(())
    }
}

/// NetEngine 示例定时任务处理器
pub struct DemoShowTimeScheduleTask {
    /// 数据库连接
    database: ModuleDatabase<BusinessModule>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,
}

impl DemoShowTimeScheduleTask {
    /// 创建示例定时任务处理器
    pub fn new(
        database: ModuleDatabase<BusinessModule>,
        identifier_generator: IdentifierGenerator,
    ) -> Self {
        Self {
            database,
            identifier_generator,
        }
    }
}

#[async_trait]
impl ScheduleTaskHandler for DemoShowTimeScheduleTask {
    async fn execute(&self, context: ScheduleTaskContext) -> Result<(), String> {
        for _ in 0..17 {
            let name = format!("张晓栋{}", Local::now().format("%S%3f"));
            if let Err(error) = enqueue_queue_task_record(
                &self.database,
                &self.identifier_generator,
                DEMO_QUEUE_SHOW_NAME,
                Some(name),
                Some(DEMO_QUEUE_SHOW_NAME_SUCCESS.to_string()),
                None,
                None,
            )
            .await
            {
                tracing::error!(target: "task_service.schedule.execute", %error, "DemoTask.WriteHello");
                return Ok(());
            }
        }

        tracing::info!(
            target: "task_service.schedule.execute",
            task_name = %context.name,
            cron = ?context.cron,
            parameter = ?context.parameter,
            "NetEngine 示例定时任务执行完成"
        );

        Ok(())
    }
}

/// NetEngine 示例主队列任务处理器
pub struct DemoShowNameQueueTask {
    /// 数据库连接
    database: ModuleDatabase<BusinessModule>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,
}

impl DemoShowNameQueueTask {
    /// 创建示例主队列任务处理器
    pub fn new(
        database: ModuleDatabase<BusinessModule>,
        identifier_generator: IdentifierGenerator,
    ) -> Self {
        Self {
            database,
            identifier_generator,
        }
    }
}

impl DemoShowNameQueueTask {
    async fn run(&self, context: QueueTaskContext) -> Result<Option<String>, String> {
        let name: Option<String> = context.deserialize_parameter()?;

        tracing::info!(
            target: "task_service.queue.execute",
            task_id = context.id,
            task_name = %context.name,
            name = %name.as_deref().unwrap_or_default(),
            "NetEngine 示例主队列任务执行中"
        );

        enqueue_queue_task_record(
            &self.database,
            &self.identifier_generator,
            DEMO_QUEUE_SEND_SMS,
            name.clone(),
            None,
            None,
            Some(context.id),
        )
        .await?;

        enqueue_queue_task_record(
            &self.database,
            &self.identifier_generator,
            DEMO_QUEUE_CALL_PHONE,
            name.clone(),
            None,
            None,
            None,
        )
        .await?;

        Ok(name)
    }
}

/// NetEngine 示例短信任务处理器
pub struct DemoSendSmsQueueTask;

#[async_trait]
impl QueueTaskHandler for DemoSendSmsQueueTask {
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let name: Option<String> = context.deserialize_parameter()?;
        let name = name.unwrap_or_default();

        tracing::info!(
            target: "task_service.queue.execute",
            task_id = context.id,
            task_name = %context.name,
            name = %name,
            "姓名：{name}，短信发送成功"
        );

        Ok(())
    }
}

/// NetEngine 示例电话任务处理器
pub struct DemoCallPhoneQueueTask;

#[async_trait]
impl QueueTaskHandler for DemoCallPhoneQueueTask {
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let name: Option<String> = context.deserialize_parameter()?;
        let name = name.unwrap_or_default();

        tracing::info!(
            target: "task_service.queue.execute",
            task_id = context.id,
            task_name = %context.name,
            name = %name,
            "姓名：{name}，打电话执行成功"
        );

        Ok(())
    }
}

/// NetEngine 示例成功回调任务处理器
pub struct DemoShowNameSuccessQueueTask;

#[async_trait]
impl QueueTaskHandler for DemoShowNameSuccessQueueTask {
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let name: Option<String> = context.deserialize_parameter()?;
        let name = name.unwrap_or_default();

        tracing::info!(
            target: "task_service.queue.execute",
            task_id = context.id,
            task_name = %context.name,
            name = %name,
            "姓名：{name}，的所有任务都执行完成了"
        );

        Ok(())
    }
}

/// 写入一条队列任务记录
async fn enqueue_queue_task_record(
    database: &ModuleDatabase<BusinessModule>,
    identifier_generator: &IdentifierGenerator,
    task_name: &str,
    parameter: Option<String>,
    callback_name: Option<String>,
    callback_parameter: Option<String>,
    parent_task_id: Option<i64>,
) -> Result<(), String> {
    QueueTaskProducer::from_module(database, identifier_generator)
        .create_single(NewQueueTask {
            name: task_name.to_string(),
            parameter: parameter
                .as_ref()
                .map(common::json_helper::object_clone_json)
                .transpose()
                .map_err(|error| error.to_string())?,
            callback_name,
            callback_parameter: callback_parameter
                .as_ref()
                .map(common::json_helper::object_clone_json)
                .transpose()
                .map_err(|error| error.to_string())?,
            parent_task_id,
            ..Default::default()
        })
        .await
        .map_err(|error| format!("写入示例队列任务失败: {error}"))?;

    Ok(())
}
