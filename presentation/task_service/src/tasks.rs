use std::sync::Arc;

use application_interface::{
    context::{AppContext, MessageContext, SmsContext},
    email::AppEmailSender,
    sms::AppSmsSender,
    storage::AppFileStorage,
};
use application_service::message::service::MessageService;
use application_service_sms::sms::send_service::SendSmsService;
use async_trait::async_trait;
use chrono::Utc;
use common::identifier_generator::IdentifierGenerator;
use repository::{queue_task, queue_task::QueueTaskStatus};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};
use source_generator_runtime::register_task_registry;
use task_service_core::registry::{
    QueueTaskContext, QueueTaskDefinition, QueueTaskHandler, ScheduleTaskContext,
    ScheduleTaskDefinition, ScheduleTaskHandler, TaskRegistry,
};
use task_service_core::{
    app_services::TaskServiceApplicationAdapter, features::TaskServiceFeatureContext,
};

/// 用户 Token 清理队列任务名称
pub const SYSTEM_QUEUE_CLEAN_USER_TOKEN: &str = "System.Queue.CleanUserToken";

/// 短信发送队列任务名称
pub const SYSTEM_QUEUE_SEND_SMS: &str = "System.Queue.SendSms";

/// 邮件发送队列任务名称
pub const SYSTEM_QUEUE_SEND_EMAIL: &str = "System.Queue.SendEmail";

/// 用户 Token 清理定时任务名称
pub const SYSTEM_SCHEDULE_CLEAN_USER_TOKEN: &str = "System.Schedule.CleanUserToken";

/// 用户 Token 清理带参定时任务名称
pub const SYSTEM_SCHEDULE_ARGS_CLEAN_USER_TOKEN: &str = "System.Schedule.CleanUserToken.Args";

/// 旧版示例队列任务名称
const LEGACY_SYSTEM_QUEUE_EXAMPLE: &str = "System.Queue.Example";

/// 旧版示例定时任务名称
const LEGACY_SYSTEM_SCHEDULE_EXAMPLE: &str = "System.Schedule.Example";

/// 旧版示例带参定时任务名称
const LEGACY_SYSTEM_SCHEDULE_ARGS_EXAMPLE: &str = "System.Schedule.ArgsExample";

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
pub struct BuiltinTaskHandlerFactory {
    /// 数据库连接
    db_pool: Arc<DatabaseConnection>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,

    /// 短信发送器
    sms_sender: Arc<dyn AppSmsSender>,

    /// 邮件发送器
    email_sender: Arc<dyn AppEmailSender>,

    /// 文件存储
    file_storage: Option<Arc<dyn AppFileStorage>>,

    /// 文件根目录列表
    file_root_paths: Arc<Vec<String>>,
}

impl BuiltinTaskHandlerFactory {
    /// 根据任务特性上下文创建处理器工厂
    pub fn new(context: &TaskServiceFeatureContext) -> Self {
        Self {
            db_pool: context.db_pool.clone(),
            identifier_generator: context.identifier_generator.clone(),
            sms_sender: context.sms_sender.clone(),
            email_sender: context.email_sender.clone(),
            file_storage: context.file_storage.clone(),
            file_root_paths: context.file_root_paths.clone(),
        }
    }

    /// 创建短信发送队列任务处理器
    pub fn send_sms_queue_task(&self) -> SendSmsQueueTask {
        SendSmsQueueTask::new(self.sms_sender.clone())
    }

    /// 创建邮件发送队列任务处理器
    pub fn send_email_queue_task(&self) -> SendEmailQueueTask {
        SendEmailQueueTask::new(
            self.db_pool.clone(),
            self.identifier_generator.clone(),
            self.email_sender.clone(),
            self.file_storage.clone(),
            self.file_root_paths.clone(),
        )
    }

    /// 创建用户 Token 清理队列任务处理器
    pub fn clean_user_token_queue_task(&self) -> CleanUserTokenQueueTask {
        CleanUserTokenQueueTask::new(self.db_pool.clone(), self.identifier_generator.clone())
    }

    /// 创建用户 Token 清理定时任务处理器
    pub fn clean_user_token_schedule_task(&self) -> CleanUserTokenScheduleTask {
        CleanUserTokenScheduleTask::new(
            self.db_pool.clone(),
            self.identifier_generator.clone(),
            None,
        )
    }

    /// 创建会投递队列的用户 Token 清理定时任务处理器
    pub fn clean_user_token_schedule_dispatcher(
        &self,
        queue_task_name: &str,
    ) -> CleanUserTokenScheduleTask {
        CleanUserTokenScheduleTask::new(
            self.db_pool.clone(),
            self.identifier_generator.clone(),
            Some(queue_task_name.to_string()),
        )
    }

    /// 创建示例定时任务处理器
    pub fn demo_show_time_schedule_task(&self) -> DemoShowTimeScheduleTask {
        DemoShowTimeScheduleTask::new(self.db_pool.clone(), self.identifier_generator.clone())
    }

    /// 创建示例主队列任务处理器
    pub fn demo_show_name_queue_task(&self) -> DemoShowNameQueueTask {
        DemoShowNameQueueTask::new(self.db_pool.clone(), self.identifier_generator.clone())
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
pub fn register_builtin_tasks(registry: &mut TaskRegistry, context: &TaskServiceFeatureContext) {
    let handlers = BuiltinTaskHandlerFactory::new(context);

    register_task_registry! {
        registry = registry;

        queue {
            definition: QueueTaskDefinition::new(SYSTEM_QUEUE_SEND_SMS)
                .with_semaphore(4)
                .with_remark("发送短信消息"),
            handler: handlers.send_sms_queue_task(),
        }

        queue {
            definition: QueueTaskDefinition::new(SYSTEM_QUEUE_SEND_EMAIL)
                .with_semaphore(4)
                .with_remark("发送邮件消息"),
            handler: handlers.send_email_queue_task(),
        }

        queue {
            definition: QueueTaskDefinition::new(SYSTEM_QUEUE_CLEAN_USER_TOKEN)
                .with_semaphore(1)
                .with_remark("清理超过保留天数的用户 Token"),
            handler: handlers.clean_user_token_queue_task(),
        }

        schedule {
            definition: ScheduleTaskDefinition::new(SYSTEM_SCHEDULE_CLEAN_USER_TOKEN)
                .with_cron("0 0 0 * * *")
                .with_remark("按默认保留策略定时清理用户 Token"),
            handler: handlers.clean_user_token_schedule_task(),
        }

        schedule {
            definition: ScheduleTaskDefinition::new(SYSTEM_SCHEDULE_ARGS_CLEAN_USER_TOKEN)
                .with_cron("0 */5 * * * *")
                .with_parameter_support()
                .with_remark("按自定义参数定时清理用户 Token"),
            handler: handlers.clean_user_token_schedule_dispatcher(SYSTEM_QUEUE_CLEAN_USER_TOKEN),
        }

        schedule {
            definition: ScheduleTaskDefinition::new(DEMO_SCHEDULE_SHOW_TIME)
                .with_cron("0/1 * * * * ?")
                .with_remark("NetEngine 示例定时任务，每秒投递示例队列任务"),
            handler: handlers.demo_show_time_schedule_task(),
        }

        queue {
            definition: QueueTaskDefinition::new(DEMO_QUEUE_SHOW_NAME)
                .with_semaphore(5)
                .with_remark("NetEngine 示例主队列任务"),
            handler: handlers.demo_show_name_queue_task(),
        }

        queue {
            definition: QueueTaskDefinition::new(DEMO_QUEUE_SEND_SMS)
                .with_semaphore(5)
                .with_remark("NetEngine 示例短信任务"),
            handler: handlers.demo_send_sms_queue_task(),
        }

        queue {
            definition: QueueTaskDefinition::new(DEMO_QUEUE_CALL_PHONE)
                .with_semaphore(5)
                .with_remark("NetEngine 示例电话任务"),
            handler: handlers.demo_call_phone_queue_task(),
        }

        queue {
            definition: QueueTaskDefinition::new(DEMO_QUEUE_SHOW_NAME_SUCCESS)
                .with_semaphore(5)
                .with_remark("NetEngine 示例成功回调任务"),
            handler: handlers.demo_show_name_success_queue_task(),
        }

        queue_alias {
            alias: LEGACY_SYSTEM_QUEUE_EXAMPLE,
            target: DEMO_QUEUE_SHOW_NAME,
        }

        schedule_alias {
            alias: LEGACY_SYSTEM_SCHEDULE_EXAMPLE,
            target: DEMO_SCHEDULE_SHOW_TIME,
        }

        schedule_alias {
            alias: LEGACY_SYSTEM_SCHEDULE_ARGS_EXAMPLE,
            target: SYSTEM_SCHEDULE_ARGS_CLEAN_USER_TOKEN,
        }
    }
}

/// 邮件发送队列任务处理器
pub struct SendEmailQueueTask {
    /// 数据库连接
    db_pool: Arc<DatabaseConnection>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,

    /// 邮件发送器
    email_sender: Arc<dyn AppEmailSender>,

    /// 文件存储
    file_storage: Option<Arc<dyn AppFileStorage>>,

    /// 文件根目录列表
    file_root_paths: Arc<Vec<String>>,
}

impl SendEmailQueueTask {
    /// 创建邮件发送队列任务处理器
    pub fn new(
        db_pool: Arc<DatabaseConnection>,
        identifier_generator: IdentifierGenerator,
        email_sender: Arc<dyn AppEmailSender>,
        file_storage: Option<Arc<dyn AppFileStorage>>,
        file_root_paths: Arc<Vec<String>>,
    ) -> Self {
        Self {
            db_pool,
            identifier_generator,
            email_sender,
            file_storage,
            file_root_paths,
        }
    }
}

#[async_trait]
impl QueueTaskHandler for SendEmailQueueTask {
    /// 执行邮件发送队列任务
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let parameter = context
            .parameter
            .as_deref()
            .ok_or_else(|| "邮件发送任务缺少参数".to_string())?;
        MessageService::new(MessageContext {
            app: AppContext {
                db: self.db_pool.as_ref(),
                id_generator: &self.identifier_generator,
                current_user: None,
            },
            root_path: self
                .file_root_paths
                .first()
                .map(String::as_str)
                .unwrap_or(""),
            attachment_root_paths: Some(self.file_root_paths.as_ref()),
            email_queue_task_name: None,
            file_storage: self.file_storage.as_deref(),
            email_sender: Some(self.email_sender.as_ref()),
        })
        .send_queue_message(parameter)
        .await
        .map_err(|error| error.message().to_string())?;

        tracing::info!(
            target: "task_service.queue.execute",
            task_name = %context.name,
            "邮件发送队列任务执行完成"
        );

        Ok(())
    }
}

/// 短信发送队列任务处理器
pub struct SendSmsQueueTask {
    /// 短信发送器
    sms_sender: Arc<dyn AppSmsSender>,
}

impl SendSmsQueueTask {
    /// 创建短信发送队列任务处理器
    pub fn new(sms_sender: Arc<dyn AppSmsSender>) -> Self {
        Self { sms_sender }
    }
}

#[async_trait]
impl QueueTaskHandler for SendSmsQueueTask {
    /// 执行短信发送队列任务
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let parameter = context
            .parameter
            .as_deref()
            .ok_or_else(|| "短信发送任务缺少参数".to_string())?;

        SendSmsService::new(SmsContext {
            sms_sender: self.sms_sender.as_ref(),
        })
        .send_queue_message(parameter)
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

/// 用户 Token 清理队列任务处理器
pub struct CleanUserTokenQueueTask {
    /// 数据库连接
    db_pool: Arc<DatabaseConnection>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,
}

impl CleanUserTokenQueueTask {
    /// 创建用户 Token 清理队列任务处理器
    pub fn new(
        db_pool: Arc<DatabaseConnection>,
        identifier_generator: IdentifierGenerator,
    ) -> Self {
        Self {
            db_pool,
            identifier_generator,
        }
    }
}

#[async_trait]
impl QueueTaskHandler for CleanUserTokenQueueTask {
    /// 执行用户 Token 清理队列任务
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let result = TaskServiceApplicationAdapter::new(&self.db_pool, &self.identifier_generator)
            .system_task()
            .clean_user_tokens(context.parameter.as_deref())
            .await
            .map_err(|error| error.message().to_string())?;

        tracing::info!(
            target: "task_service.queue.execute",
            task_name = %context.name,
            parameter = ?context.parameter,
            deleted_count = result.deleted_count,
            retain_days = result.retain_days,
            batch_size = result.batch_size,
            "用户 Token 清理队列任务执行完成"
        );
        Ok(())
    }
}

/// 用户 Token 清理定时任务处理器
pub struct CleanUserTokenScheduleTask {
    /// 数据库连接
    db_pool: Arc<DatabaseConnection>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,

    /// 默认投递的队列任务名称
    queue_task_name: Option<String>,
}

impl CleanUserTokenScheduleTask {
    /// 创建用户 Token 清理定时任务处理器
    pub fn new(
        db_pool: Arc<DatabaseConnection>,
        identifier_generator: IdentifierGenerator,
        queue_task_name: Option<String>,
    ) -> Self {
        Self {
            db_pool,
            identifier_generator,
            queue_task_name,
        }
    }
}

#[async_trait]
impl ScheduleTaskHandler for CleanUserTokenScheduleTask {
    /// 执行用户 Token 清理定时任务
    async fn execute(&self, context: ScheduleTaskContext) -> Result<(), String> {
        if let Some(queue_task_name) = &self.queue_task_name {
            TaskServiceApplicationAdapter::new(&self.db_pool, &self.identifier_generator)
                .system_task()
                .enqueue_queue_task(queue_task_name, context.parameter.clone())
                .await
                .map_err(|error| error.message().to_string())?;

            tracing::info!(
                target: "task_service.schedule.execute",
                task_name = %context.name,
                queue_task_name = %queue_task_name,
                parameter = ?context.parameter,
                cron = ?context.cron,
                "用户 Token 清理定时任务已投递队列任务"
            );

            return Ok(());
        }

        let result = TaskServiceApplicationAdapter::new(&self.db_pool, &self.identifier_generator)
            .system_task()
            .clean_user_tokens(context.parameter.as_deref())
            .await
            .map_err(|error| error.message().to_string())?;

        tracing::info!(
            target: "task_service.schedule.execute",
            task_name = %context.name,
            parameter = ?context.parameter,
            cron = ?context.cron,
            deleted_count = result.deleted_count,
            retain_days = result.retain_days,
            batch_size = result.batch_size,
            "用户 Token 清理定时任务执行完成"
        );
        Ok(())
    }
}

/// NetEngine 示例定时任务处理器
pub struct DemoShowTimeScheduleTask {
    /// 数据库连接
    db_pool: Arc<DatabaseConnection>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,
}

impl DemoShowTimeScheduleTask {
    /// 创建示例定时任务处理器
    pub fn new(
        db_pool: Arc<DatabaseConnection>,
        identifier_generator: IdentifierGenerator,
    ) -> Self {
        Self {
            db_pool,
            identifier_generator,
        }
    }
}

#[async_trait]
impl ScheduleTaskHandler for DemoShowTimeScheduleTask {
    async fn execute(&self, context: ScheduleTaskContext) -> Result<(), String> {
        for index in 0..3 {
            let name = format!("张晓栋{}{:03}", Utc::now().format("%S%3f"), index);
            enqueue_queue_task_record(
                self.db_pool.as_ref(),
                &self.identifier_generator,
                DEMO_QUEUE_SHOW_NAME,
                Some(name.clone()),
                Some(DEMO_QUEUE_SHOW_NAME_SUCCESS.to_string()),
                Some(name),
                None,
            )
            .await?;
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
    db_pool: Arc<DatabaseConnection>,

    /// ID 生成器
    identifier_generator: IdentifierGenerator,
}

impl DemoShowNameQueueTask {
    /// 创建示例主队列任务处理器
    pub fn new(
        db_pool: Arc<DatabaseConnection>,
        identifier_generator: IdentifierGenerator,
    ) -> Self {
        Self {
            db_pool,
            identifier_generator,
        }
    }
}

#[async_trait]
impl QueueTaskHandler for DemoShowNameQueueTask {
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let name = context
            .parameter
            .clone()
            .ok_or_else(|| "ShowName 任务缺少参数".to_string())?;

        tracing::info!(
            target: "task_service.queue.execute",
            task_id = context.id,
            task_name = %context.name,
            name = %name,
            "NetEngine 示例主队列任务执行中"
        );

        enqueue_queue_task_record(
            self.db_pool.as_ref(),
            &self.identifier_generator,
            DEMO_QUEUE_SEND_SMS,
            Some(name.clone()),
            None,
            None,
            Some(context.id),
        )
        .await?;

        enqueue_queue_task_record(
            self.db_pool.as_ref(),
            &self.identifier_generator,
            DEMO_QUEUE_CALL_PHONE,
            Some(name),
            None,
            None,
            Some(context.id),
        )
        .await?;

        Ok(())
    }
}

/// NetEngine 示例短信任务处理器
pub struct DemoSendSmsQueueTask;

#[async_trait]
impl QueueTaskHandler for DemoSendSmsQueueTask {
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String> {
        let name = context
            .parameter
            .as_deref()
            .ok_or_else(|| "SendSMS 任务缺少参数".to_string())?;

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
        let name = context
            .parameter
            .as_deref()
            .ok_or_else(|| "CallPhone 任务缺少参数".to_string())?;

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
        let name = context
            .parameter
            .as_deref()
            .ok_or_else(|| "ShowNameSuccess 任务缺少参数".to_string())?;

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
    db: &DatabaseConnection,
    identifier_generator: &IdentifierGenerator,
    task_name: &str,
    parameter: Option<String>,
    callback_name: Option<String>,
    callback_parameter: Option<String>,
    parent_task_id: Option<i64>,
) -> Result<(), String> {
    let queue_task_id = identifier_generator
        .get_id()
        .await
        .map_err(|error| format!("生成示例队列任务主键失败: {error}"))?;

    queue_task::ActiveModel {
        id: Set(queue_task_id),
        name: Set(task_name.to_string()),
        parameter: Set(parameter),
        status: Set(QueueTaskStatus::Pending),
        count: Set(0),
        first_time: Set(None),
        last_time: Set(None),
        success_time: Set(None),
        create_time: Set(Utc::now().into()),
        delete_time: Set(None),
        plan_time: Set(None),
        worker_id: Set(None),
        lease_expire_time: Set(None),
        callback_name: Set(callback_name),
        callback_parameter: Set(callback_parameter),
        child_success_time: Set(None),
        parent_task_id: Set(parent_task_id),
    }
    .insert(db)
    .await
    .map_err(|error| format!("写入示例队列任务失败: {error}"))?;

    Ok(())
}
