use crate::registry::{RegisteredTaskKind, TaskRegistry};
use event_bus::CancellationToken;
use queue_task_database::{
    ARGS_DEFAULT_PARAMETER, QueueTaskStore, TaskSettingDefinition, task_setting,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
    time::Duration,
};

#[derive(Clone)]
pub(crate) struct QueueConfiguration {
    pub name: String,
    pub semaphore: i32,
    pub is_enable: bool,
}

#[derive(Clone)]
pub(crate) struct ScheduleConfiguration {
    pub id: Option<i64>,
    /// 参数实例使用“定义名:配置主键”，无参任务使用定义名。
    pub name: String,
    pub definition_name: String,
    pub parameter: Option<String>,
    pub cron: Option<String>,
    pub is_enable: bool,
}

#[derive(Default)]
struct ConfigurationSnapshot {
    queue: BTreeMap<String, QueueConfiguration>,
    schedule: BTreeMap<String, ScheduleConfiguration>,
}

/// 调度读取内存快照；数据库同步与 Debug 进程选择共用此运行配置。
#[derive(Clone, Default)]
pub(crate) struct TaskRuntimeConfiguration(Arc<RwLock<ConfigurationSnapshot>>);

impl TaskRuntimeConfiguration {
    pub fn initialize(registry: &TaskRegistry, settings: &[task_setting::Model]) -> Self {
        let result = Self::default();
        result.apply(registry, settings, true);
        if !cfg!(debug_assertions) {
            result.apply(registry, settings, false);
        }
        result
    }

    fn apply(&self, registry: &TaskRegistry, settings: &[task_setting::Model], debug: bool) {
        let mut state = self.0.write().unwrap_or_else(|error| error.into_inner());
        // 保留已发现的带参实例，删除配置后禁用它，而不是改变运行身份。
        for task in state.schedule.values_mut() {
            task.is_enable = false;
        }
        for definition in registry.queue_definitions() {
            let setting = settings
                .iter()
                .find(|item| item.category == "QueueTask" && item.name == definition.name);
            let task = QueueConfiguration {
                name: definition.name.clone(),
                semaphore: if debug {
                    definition.semaphore
                } else {
                    setting
                        .and_then(|item| item.semaphore)
                        .unwrap_or(definition.semaphore)
                },
                is_enable: !debug && setting.is_some_and(|item| item.is_enable),
            };
            state.queue.insert(task.name.clone(), task);
        }
        for definition in registry.schedule_definitions() {
            if definition.supports_parameter {
                for setting in settings.iter().filter(|item| {
                    item.category == "ScheduleTask"
                        && item.name == definition.name
                        && item.parameter.is_some()
                        && item.parameter.as_deref() != Some(ARGS_DEFAULT_PARAMETER)
                }) {
                    let name = format!("{}:{}", definition.name, setting.id);
                    // 初始化会发现全部参数实例；后续仅自动发现新启用的实例。
                    if !debug && !setting.is_enable && !state.schedule.contains_key(&name) {
                        continue;
                    }
                    state.schedule.insert(
                        name.clone(),
                        ScheduleConfiguration {
                            id: Some(setting.id),
                            name,
                            definition_name: definition.name.clone(),
                            parameter: setting.parameter.clone(),
                            cron: resolved_cron(&setting.cron, &definition.cron),
                            is_enable: !debug && setting.is_enable,
                        },
                    );
                }
            } else {
                let setting = settings.iter().find(|item| {
                    item.category == "ScheduleTask"
                        && item.name == definition.name
                        && item.parameter.is_none()
                });
                state.schedule.insert(
                    definition.name.clone(),
                    ScheduleConfiguration {
                        id: setting.map(|item| item.id),
                        name: definition.name.clone(),
                        definition_name: definition.name.clone(),
                        parameter: None,
                        cron: if debug {
                            definition.cron.clone()
                        } else {
                            resolved_cron(
                                &setting.and_then(|item| item.cron.clone()),
                                &definition.cron,
                            )
                        },
                        is_enable: !debug && setting.is_some_and(|item| item.is_enable),
                    },
                );
            }
        }
    }

    pub fn queue(&self) -> Vec<QueueConfiguration> {
        self.0
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .queue
            .values()
            .filter(|item| item.is_enable)
            .cloned()
            .collect()
    }

    pub fn schedules(&self) -> Vec<ScheduleConfiguration> {
        self.0
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .schedule
            .values()
            .filter(|item| item.is_enable)
            .cloned()
            .collect()
    }

    pub fn disable_schedule(&self, name: &str) {
        if let Some(task) = self
            .0
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .schedule
            .get_mut(name)
        {
            task.is_enable = false;
        }
    }

    pub fn menu(&self) -> Vec<(RegisteredTaskKind, String, bool)> {
        let state = self.0.read().unwrap_or_else(|error| error.into_inner());
        state
            .queue
            .values()
            .map(|task| (RegisteredTaskKind::Queue, task.name.clone(), task.is_enable))
            .chain(state.schedule.values().map(|task| {
                (
                    RegisteredTaskKind::Schedule,
                    task.name.clone(),
                    task.is_enable,
                )
            }))
            .collect()
    }

    pub fn enable(&self, kind: RegisteredTaskKind, name: &str) {
        let mut state = self.0.write().unwrap_or_else(|error| error.into_inner());
        match kind {
            RegisteredTaskKind::Queue => {
                if let Some(task) = state.queue.get_mut(name) {
                    task.is_enable = true;
                }
            }
            RegisteredTaskKind::Schedule => {
                if let Some(task) = state.schedule.get_mut(name) {
                    task.is_enable = true;
                }
            }
        }
    }

    pub async fn synchronize(
        &self,
        store: &dyn QueueTaskStore,
        registry: &TaskRegistry,
        stopping: CancellationToken,
    ) {
        if cfg!(debug_assertions) || registry.registered_tasks().is_empty() {
            stopping.cancelled().await;
            return;
        }
        loop {
            tokio::select! {
                biased;
                _ = stopping.cancelled() => return,
                result = self.refresh(store, registry) => {
                    if let Err(error) = result { tracing::error!(target: "task_service.registry.sync", %error, "任务配置读取或定义补充失败，下轮重试"); }
                }
            }
            tokio::select! {
                _ = stopping.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(60)) => {},
            }
        }
    }

    async fn refresh(
        &self,
        store: &dyn QueueTaskStore,
        registry: &TaskRegistry,
    ) -> Result<(), sea_orm::DbErr> {
        let settings = store.get_settings().await?;
        self.apply(registry, &settings, false);
        store.ensure_settings(&definitions(registry)).await
    }
}

fn resolved_cron(value: &Option<String>, default: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .or(default.as_ref())
        .cloned()
}

fn definitions(registry: &TaskRegistry) -> Vec<TaskSettingDefinition> {
    registry
        .queue_definitions()
        .into_iter()
        .map(|definition| TaskSettingDefinition {
            name: definition.name,
            category: "QueueTask".into(),
            semaphore: Some(definition.semaphore),
            cron: None,
            remark: definition.remark,
            parameter: None,
        })
        .chain(
            registry
                .schedule_definitions()
                .into_iter()
                .map(|definition| TaskSettingDefinition {
                    name: definition.name,
                    category: "ScheduleTask".into(),
                    semaphore: None,
                    cron: definition.cron,
                    remark: definition.remark,
                    parameter: definition
                        .supports_parameter
                        .then(|| ARGS_DEFAULT_PARAMETER.to_string()),
                }),
        )
        .collect()
}
