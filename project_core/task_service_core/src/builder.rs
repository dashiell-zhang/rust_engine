use std::sync::Arc;

use crate::{
    BuildTaskServiceCache, BuildTaskServiceLock, BuildTaskServiceLoggerRuntime,
    TaskServiceLoggerBuilderContext,
    features::{TaskServiceFeature, TaskServiceFeatureContext},
    registry::TaskRegistry,
    runner::{TaskServiceRunner, TaskServiceRunnerOptions},
    settings::Settings,
    sync::TaskRuntimeConfiguration,
};
use common::identifier_generator::IdentifierGenerator;
use logger::LoggerRuntime;
use queue_task_database::{DatabaseQueueTaskStore, QueueTaskStore};
use repository_core::ReadDatabaseConnection;

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
    build_event_transport: Option<crate::BuildTaskServiceEventTransport>,
    build_databases: Option<crate::BuildTaskServiceDatabases>,

    build_resources: Option<crate::BuildTaskServiceResources>,

    /// 已注册任务特性
    features: Vec<Box<dyn TaskServiceFeature>>,
}

/// TaskService 宿主实例
pub struct TaskServiceHost {
    /// 任务运行器
    runner: TaskServiceRunner,

    /// 数据库只读连接
    read_db_pool: Arc<ReadDatabaseConnection>,

    /// 当前进程的任务运行配置
    configuration: TaskRuntimeConfiguration,

    /// 日志运行时守卫
    _logger_runtime: LoggerRuntime,
    event_bus: event_bus::PreparedEventBus,
    event_listeners: event_bus::EventListenerRegistry,
}

impl TaskServiceHostBuilder {
    /// 创建 TaskService 宿主构建器
    pub fn new(
        settings: Settings,
        build_distributed_lock: BuildTaskServiceLock,
        build_distributed_cache: BuildTaskServiceCache,
        build_logger_runtime: BuildTaskServiceLoggerRuntime,
    ) -> Self {
        Self {
            settings,
            build_distributed_lock,
            build_distributed_cache,
            build_logger_runtime,
            features: Vec::new(),
            build_event_transport: None,
            build_databases: None,
            build_resources: None,
        }
    }

    pub fn with_resources_factory(
        mut self,
        factory: Option<crate::BuildTaskServiceResources>,
    ) -> Self {
        self.build_resources = factory;
        self
    }

    pub fn with_databases_factory(mut self, factory: crate::BuildTaskServiceDatabases) -> Self {
        self.build_databases = Some(factory);
        self
    }

    /// 运行入口指定事件后端，禁止公共宿主自行选择实现。
    pub fn with_event_transport_factory(
        mut self,
        factory: crate::BuildTaskServiceEventTransport,
    ) -> Self {
        self.build_event_transport = Some(factory);
        self
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
        let identifier_generator = IdentifierGenerator::from_settings(
            &self.settings.identifier_generator,
            &self.settings.cache.connection,
        )
        .await
        .map_err(|error| format!("ID 服务初始化失败: {error}"))?;

        let databases = self.build_databases.ok_or("任务宿主未指定数据库模块装配")?(
            &self.settings,
        )
        .await?;
        let db_pool = databases.queue_write.clone();
        let read_db_pool = databases.queue_read.clone();

        let logger_runtime = (self.build_logger_runtime)(
            &self.settings,
            &TaskServiceLoggerBuilderContext {
                identifier_generator: identifier_generator.clone(),
            },
        )
        .await?;

        let build_event_transport = self.build_event_transport.ok_or("宿主未注册事件后端")?;
        let transport = build_event_transport(&self.settings, &identifier_generator)?;
        let event_publisher = event_bus::EventPublisher::new(transport.clone());
        let startup_stopping = event_bus::CancellationToken::new();
        let prepare = event_bus::PreparedEventBus::prepare(
            transport,
            self.settings.event_bus.clone(),
            &startup_stopping,
        );
        tokio::pin!(prepare);
        let event_bus = tokio::select! {
            result = &mut prepare => result?,
            signal = common::system_helper::wait_for_shutdown() => {
                startup_stopping.cancel();
                // 让适配器完成服务器端取消，仍遵守宿主三十秒停机上限。
                let _ = tokio::time::timeout(std::time::Duration::from_secs(30), &mut prepare).await;
                signal?;
                return Err("事件总线启动已取消".to_string());
            },
        };
        let mut event_listeners = event_bus::EventListenerRegistry::default();

        let resources = match self.build_resources {
            Some(build) => build(&self.settings).await?,
            None => common::resources::Resources::default(),
        };
        let feature_context = TaskServiceFeatureContext {
            cache: distributed_cache,
            event_publisher,
            modules: databases.modules,
            identifier_generator: identifier_generator.clone(),
            resources: Arc::new(resources),
        };
        for feature in &self.features {
            feature.register_event_listeners(&mut event_listeners, &feature_context)?;
        }
        let registry = build_task_registry(self.features, &feature_context)?;

        let store: Arc<dyn QueueTaskStore> = Arc::new(DatabaseQueueTaskStore::new(
            db_pool.clone(),
            identifier_generator,
        ));
        let settings = store
            .get_settings()
            .await
            .map_err(|error| format!("任务配置初始化失败: {error}"))?;
        let configuration = TaskRuntimeConfiguration::initialize(&registry, &settings);

        let runner = TaskServiceRunner::new(
            store,
            configuration.clone(),
            registry,
            distributed_lock,
            TaskServiceRunnerOptions {
                queue_poll_interval_seconds: self.settings.poll.queue_interval_seconds,
                schedule_poll_interval_milliseconds: self
                    .settings
                    .poll
                    .resolved_schedule_interval_milliseconds(),
                queue_lock_expiry_seconds: self.settings.lock.expiry_seconds,
            },
        );

        Ok(TaskServiceHost {
            runner,
            read_db_pool,
            configuration,
            _logger_runtime: logger_runtime,
            event_bus,
            event_listeners,
        })
    }
}

impl TaskServiceHost {
    /// 获取只读数据库连接
    pub fn read_db(&self) -> &ReadDatabaseConnection {
        self.read_db_pool.as_ref()
    }

    pub async fn run(mut self) -> Result<(), String> {
        tracing::info!(target: "task_service.startup", "TaskService 启动完成");
        let mut events = self.event_bus.start(self.event_listeners);
        let stopping = event_bus::CancellationToken::new();
        let mut runner = Box::pin(self.runner.run(stopping.clone()));
        let mut menu = Box::pin(crate::debug_menu::run(self.configuration));
        let mut menu_done = false;
        let (mut result, drain) = loop {
            tokio::select! {
                _ = &mut runner => break (Ok(()), false),
                failure = events.wait_failure() => break (failure, true),
                signal = common::system_helper::wait_for_shutdown() => break (signal, true),
                selection = &mut menu, if !menu_done => {
                    match selection {
                        Ok(true) => menu_done = true,
                        Ok(false) => break (Ok(()), true),
                        Err(error) => break (Err(error), true),
                    }
                }
            }
        };
        drop(menu);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        stopping.cancel();
        events.cancel();
        if drain
            && tokio::time::timeout_at(deadline, &mut runner)
                .await
                .is_err()
        {
            tracing::warn!(target: "task_service.shutdown", "任务排空期限耗尽，剩余队列记录等待租约恢复");
            result = result.and(Err("任务停机等待超时".into()));
        }
        // 丢弃拥有 JoinSet 的运行器会取消剩余执行，禁止超时后继续续租或提交。
        drop(runner);
        let _ = tokio::time::timeout_at(deadline, events.shutdown()).await;
        drop(events);
        self._logger_runtime
            .shutdown(deadline.saturating_duration_since(tokio::time::Instant::now()))
            .await;
        result
    }
}

/// 创建任务注册表
fn build_task_registry(
    features: Vec<Box<dyn TaskServiceFeature>>,
    context: &TaskServiceFeatureContext,
) -> Result<TaskRegistry, String> {
    let mut registry = TaskRegistry::new();

    for feature in features {
        feature.register(&mut registry, context)?;
    }
    registry.resolve_aliases()?;

    Ok(registry)
}
