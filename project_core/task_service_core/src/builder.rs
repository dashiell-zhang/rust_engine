use std::{collections::BTreeMap, io, sync::Arc};

use crate::{
    BuildTaskServiceCache, BuildTaskServiceEmailSender, BuildTaskServiceFileStorage,
    BuildTaskServiceLock, BuildTaskServiceLoggerRuntime, BuildTaskServiceSmsSender,
    TaskServiceLoggerBuilderContext,
    features::{TaskServiceFeature, TaskServiceFeatureContext},
    registry::{RegisteredTaskKind, TaskRegistry},
    runner::{TaskServiceRunner, TaskServiceRunnerOptions},
    settings::Settings,
    startup::TaskServiceStartup,
    sync::TaskRegistrySynchronizer,
};
use application_model::task_center::ARGS_DEFAULT_PARAMETER;
use common::identifier_generator::IdentifierGenerator;
use identifier_generator::refresh::RefreshSignTask;
use logger::LoggerRuntime;
use repository::{extensions::SoftDeleteQueryExt, task_setting};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, Database, DatabaseConnection, EntityTrait, IntoActiveModel,
};

/// Debug 菜单展示项
struct DebugTaskMenuItem {
    /// 菜单序号
    index: usize,

    /// 任务配置主键
    task_id: i64,

    /// 任务类型
    kind: RegisteredTaskKind,

    /// 任务名称
    name: String,

    /// 当前是否启用
    is_enable: bool,
}

/// TaskService 宿主构建器
pub struct TaskServiceHostBuilder {
    /// 宿主配置
    settings: Settings,

    /// 分布式锁构建方法
    build_distributed_lock: BuildTaskServiceLock,

    /// 分布式缓存构建方法
    build_distributed_cache: BuildTaskServiceCache,

    /// 日志运行时构建方法
    build_logger_runtime: BuildTaskServiceLoggerRuntime,

    /// 短信发送器构建方法
    build_sms_sender: BuildTaskServiceSmsSender,

    /// 邮件发送器构建方法
    build_email_sender: BuildTaskServiceEmailSender,

    /// 文件存储构建方法
    build_file_storage: BuildTaskServiceFileStorage,

    /// 已注册任务特性
    features: Vec<Box<dyn TaskServiceFeature>>,
}

/// TaskService 宿主实例
pub struct TaskServiceHost {
    /// 任务运行器
    runner: TaskServiceRunner,

    /// 数据库连接
    db_pool: Arc<DatabaseConnection>,

    /// 任务注册表
    registry: TaskRegistry,

    /// 日志运行时守卫
    _logger_runtime: LoggerRuntime,
}

impl TaskServiceHostBuilder {
    /// 创建 TaskService 宿主构建器
    pub fn new(
        settings: Settings,
        build_distributed_lock: BuildTaskServiceLock,
        build_distributed_cache: BuildTaskServiceCache,
        build_logger_runtime: BuildTaskServiceLoggerRuntime,
        build_sms_sender: BuildTaskServiceSmsSender,
        build_email_sender: BuildTaskServiceEmailSender,
        build_file_storage: BuildTaskServiceFileStorage,
    ) -> Self {
        Self {
            settings,
            build_distributed_lock,
            build_distributed_cache,
            build_logger_runtime,
            build_sms_sender,
            build_email_sender,
            build_file_storage,
            features: Vec::new(),
        }
    }

    /// 注册单个任务特性
    pub fn with_feature(mut self, feature: Box<dyn TaskServiceFeature>) -> Self {
        self.features.push(feature);
        self
    }

    /// 注册任务特性集合
    pub fn with_features(mut self, features: Vec<Box<dyn TaskServiceFeature>>) -> Self {
        for feature in features {
            self = self.with_feature(feature);
        }

        self
    }

    /// 构建 TaskService 宿主实例
    pub async fn build(self) -> Result<TaskServiceHost, String> {
        let distributed_lock = (self.build_distributed_lock)(&self.settings).await?;
        let distributed_cache = (self.build_distributed_cache)(&self.settings).await?;
        let identifier_generator = IdentifierGenerator::from_infrastructure(
            &self.settings.identifier_generator,
            Arc::clone(&distributed_lock),
            Arc::clone(&distributed_cache),
        )
        .await
        .map_err(|error| format!("ID 服务初始化失败: {error}"))?;
        let should_refresh_identifier_sign = identifier_generator.requires_sign_refresh().await;

        let db_pool = Arc::new(
            Database::connect(TaskServiceStartup::build_postgres_connect_options(
                self.settings.database.connection.clone(),
                &self.settings.host.database_pool,
            ))
            .await
            .map_err(|error| format!("数据库连接初始化失败: {error}"))?,
        );

        let logger_runtime = (self.build_logger_runtime)(
            &self.settings,
            &TaskServiceLoggerBuilderContext {
                db_pool: Arc::clone(&db_pool),
                identifier_generator: identifier_generator.clone(),
            },
        )?;

        let sms_sender = (self.build_sms_sender)(&self.settings)?;
        let email_sender = (self.build_email_sender)(&self.settings)?;
        let file_storage = (self.build_file_storage)(&self.settings)?;
        let mut file_root_paths = Vec::new();
        let primary_root_path = self.settings.file.root_path.trim();
        if !primary_root_path.is_empty() {
            file_root_paths.push(primary_root_path.to_string());
        }

        for root_path in &self.settings.file.additional_root_paths {
            let root_path = root_path.trim();
            if root_path.is_empty() {
                continue;
            }

            if file_root_paths.iter().any(|item| item == root_path) {
                continue;
            }

            file_root_paths.push(root_path.to_string());
        }

        let file_root_paths = Arc::new(file_root_paths);

        let feature_context = TaskServiceFeatureContext {
            db_pool: Arc::clone(&db_pool),
            identifier_generator: identifier_generator.clone(),
            sms_sender,
            email_sender,
            file_storage,
            file_root_paths,
        };
        let registry = build_task_registry(self.features, &feature_context);

        sync_task_registry(&db_pool, &registry, &identifier_generator).await;

        let runner = TaskServiceRunner::new(
            Arc::clone(&db_pool),
            registry.clone(),
            distributed_lock,
            identifier_generator.clone(),
            TaskServiceRunnerOptions {
                queue_poll_interval_seconds: self.settings.poll.queue_interval_seconds,
                schedule_poll_interval_milliseconds: self
                    .settings
                    .poll
                    .resolved_schedule_interval_milliseconds(),
                queue_lock_expiry_seconds: self.settings.lock.expiry_seconds,
                schedule_lock_expiry_seconds: self.settings.lock.expiry_seconds,
                log_project: self.settings.logger.project.clone(),
            },
        );

        if should_refresh_identifier_sign {
            RefreshSignTask::new(
                Arc::new(identifier_generator.clone()),
                Arc::clone(&distributed_cache),
            )
            .run()
            .await;
        }

        Ok(TaskServiceHost {
            runner,
            db_pool,
            registry,
            _logger_runtime: logger_runtime,
        })
    }
}

impl TaskServiceHost {
    /// Debug 模式下选择需要启用的任务
    pub async fn select_debug_tasks(&self) -> Result<bool, String> {
        if !cfg!(debug_assertions) {
            return Ok(true);
        }

        loop {
            let items = self.load_debug_task_menu_items().await?;

            println!();
            println!("TaskService Debug 模式");
            println!("当前已完成任务注册表同步");
            println!();

            for item in &items {
                let action_status = if item.is_enable {
                    "已启用"
                } else {
                    "未启用"
                };
                println!(
                    "[{}] {}：{} {}",
                    item.index,
                    item.kind.display_name(),
                    item.name,
                    action_status,
                );
            }

            println!();
            println!("请输入空格分隔的序号启用任务");
            println!("直接回车刷新列表 输入 start 启动当前选择 输入 skip 跳过启动");

            let mut input = String::new();
            match io::stdin().read_line(&mut input) {
                Ok(0) => return Ok(true),
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(
                        target: "task_service.debug.menu",
                        error = %error,
                        "读取 Debug 菜单输入失败，已按默认方式继续启动"
                    );
                    return Ok(true);
                }
            }

            let command = input.trim();

            if command.is_empty() {
                continue;
            }

            if command.eq_ignore_ascii_case("start")
                || command.eq_ignore_ascii_case("run")
                || command.eq_ignore_ascii_case("y")
                || command.eq_ignore_ascii_case("yes")
            {
                return Ok(true);
            }

            if command.eq_ignore_ascii_case("skip")
                || command.eq_ignore_ascii_case("n")
                || command.eq_ignore_ascii_case("no")
                || command.eq_ignore_ascii_case("q")
                || command.eq_ignore_ascii_case("quit")
            {
                return Ok(false);
            }

            let selected_task_ids = parse_selected_task_ids(command, &items)?;

            if selected_task_ids.is_empty() {
                println!("未选择任何有效任务");
                continue;
            }

            self.enable_debug_tasks(&selected_task_ids).await?;
            println!("已启用 {} 个任务，回车刷新列表", selected_task_ids.len());
        }
    }

    /// 启动 TaskService 宿主
    pub async fn run(self) {
        tracing::info!(target: "task_service.startup", "TaskService 启动完成");
        self.runner.run().await;
    }

    /// 加载 Debug 菜单展示项
    async fn load_debug_task_menu_items(&self) -> Result<Vec<DebugTaskMenuItem>, String> {
        let models = task_setting::Entity::find()
            .not_deleted(task_setting::Column::DeleteTime)
            .all(self.db_pool.as_ref())
            .await
            .map_err(|error| format!("加载任务配置失败: {error}"))?;

        let mut model_index = BTreeMap::new();

        for model in models {
            let kind = match model.category.as_str() {
                "QueueTask" => RegisteredTaskKind::Queue,
                "ScheduleTask" => RegisteredTaskKind::Schedule,
                _ => continue,
            };

            let parameter_key = model.parameter.clone().unwrap_or_default();
            model_index.insert(
                (
                    kind.category_name().to_string(),
                    model.name.clone(),
                    parameter_key,
                ),
                model,
            );
        }

        let mut items = Vec::new();

        for (index, task) in self.registry.registered_tasks().into_iter().enumerate() {
            let parameter_key =
                if task.kind == RegisteredTaskKind::Schedule && task.supports_parameter {
                    ARGS_DEFAULT_PARAMETER.to_string()
                } else {
                    String::new()
                };

            let Some(model) = model_index.get(&(
                task.kind.category_name().to_string(),
                task.name.clone(),
                parameter_key,
            )) else {
                return Err(format!(
                    "任务配置未同步到数据库: {} {}",
                    task.kind.display_name(),
                    task.name
                ));
            };

            items.push(DebugTaskMenuItem {
                index: index + 1,
                task_id: model.id,
                kind: task.kind,
                name: task.name,
                is_enable: model.is_enable,
            });
        }

        Ok(items)
    }

    /// 启用 Debug 菜单中选中的任务
    async fn enable_debug_tasks(&self, task_ids: &[i64]) -> Result<(), String> {
        for task_id in task_ids {
            let Some(task_model) = task_setting::Entity::find_by_id(*task_id)
                .one(self.db_pool.as_ref())
                .await
                .map_err(|error| format!("加载任务配置失败: {error}"))?
            else {
                return Err(format!("任务配置不存在: {task_id}"));
            };

            if task_model.is_enable {
                continue;
            }

            let mut active = task_model.into_active_model();
            active.is_enable = Set(true);
            active
                .update(self.db_pool.as_ref())
                .await
                .map_err(|error| format!("更新任务启用状态失败: {error}"))?;
        }

        Ok(())
    }
}

/// 解析 Debug 菜单中输入的任务序号
fn parse_selected_task_ids(command: &str, items: &[DebugTaskMenuItem]) -> Result<Vec<i64>, String> {
    let item_index = items
        .iter()
        .map(|item| (item.index, item.task_id))
        .collect::<BTreeMap<_, _>>();

    let mut task_ids = Vec::new();

    for token in command.split_whitespace() {
        let index = token
            .parse::<usize>()
            .map_err(|_| format!("无效的任务序号: {token}"))?;

        let Some(task_id) = item_index.get(&index) else {
            return Err(format!("无效的任务序号: {token}"));
        };

        if !task_ids.contains(task_id) {
            task_ids.push(*task_id);
        }
    }

    Ok(task_ids)
}

/// 创建任务注册表
fn build_task_registry(
    features: Vec<Box<dyn TaskServiceFeature>>,
    context: &TaskServiceFeatureContext,
) -> TaskRegistry {
    let mut registry = TaskRegistry::new();

    for feature in features {
        feature.register(&mut registry, context);
    }

    registry
}

/// 同步任务注册表到数据库
async fn sync_task_registry(
    db_pool: &DatabaseConnection,
    registry: &TaskRegistry,
    identifier_generator: &IdentifierGenerator,
) {
    match TaskRegistrySynchronizer::sync_task_settings(db_pool, registry, identifier_generator)
        .await
    {
        Ok(result) => {
            tracing::info!(
                target: "task_service.registry.sync",
                created_queue_names = ?result.created_queue_names,
                existing_queue_names = ?result.existing_queue_names,
                created_schedule_names = ?result.created_schedule_names,
                existing_schedule_names = ?result.existing_schedule_names,
                "任务定义同步完成"
            );
        }
        Err(error) => {
            tracing::error!(
                target: "task_service.registry.sync",
                error = %error,
                "任务定义同步失败"
            );
        }
    }
}
