use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;

/// 已注册任务类型
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RegisteredTaskKind {
    /// 队列任务
    Queue,

    /// 定时任务
    Schedule,
}

impl RegisteredTaskKind {
    /// 获取任务分类名称
    pub fn category_name(&self) -> &'static str {
        match self {
            Self::Queue => "QueueTask",
            Self::Schedule => "ScheduleTask",
        }
    }

    /// 获取任务分类展示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Queue => "队列任务",
            Self::Schedule => "定时任务",
        }
    }
}

/// 已注册任务信息
#[derive(Clone)]
pub struct RegisteredTaskDefinition {
    /// 任务类型
    pub kind: RegisteredTaskKind,

    /// 任务名称
    pub name: String,

    /// 是否支持参数
    pub supports_parameter: bool,
}

/// 队列任务执行上下文模型
#[derive(Clone)]
pub struct QueueTaskContext {
    /// 任务 ID
    pub id: i64,

    /// 任务名称
    pub name: String,

    /// 任务参数
    pub parameter: Option<String>,
}

/// 定时任务执行上下文模型
#[derive(Clone)]
pub struct ScheduleTaskContext {
    /// 任务名称
    pub name: String,

    /// 任务参数
    pub parameter: Option<String>,

    /// Cron 表达式
    pub cron: Option<String>,
}

#[async_trait]
/// 队列任务处理器
pub trait QueueTaskHandler: Send + Sync {
    /// 执行队列任务
    async fn execute(&self, context: QueueTaskContext) -> Result<(), String>;
}

#[async_trait]
/// 定时任务处理器
pub trait ScheduleTaskHandler: Send + Sync {
    /// 执行定时任务
    async fn execute(&self, context: ScheduleTaskContext) -> Result<(), String>;
}

/// 队列任务定义
#[derive(Clone)]
pub struct QueueTaskDefinition {
    /// 任务名称
    pub name: String,

    /// 默认并发信号量
    pub semaphore: i32,

    /// 备注
    pub remark: Option<String>,
}

impl QueueTaskDefinition {
    /// 创建队列任务定义
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            semaphore: 1,
            remark: None,
        }
    }

    /// 设置默认并发信号量
    pub fn with_semaphore(mut self, semaphore: i32) -> Self {
        self.semaphore = semaphore;
        self
    }

    /// 设置任务备注
    pub fn with_remark(mut self, remark: impl Into<String>) -> Self {
        self.remark = Some(remark.into());
        self
    }
}

/// 定时任务定义
#[derive(Clone)]
pub struct ScheduleTaskDefinition {
    /// 任务名称
    pub name: String,

    /// 默认 Cron 表达式
    pub cron: Option<String>,

    /// 是否支持参数
    pub supports_parameter: bool,

    /// 上次执行未完成时跳过本次触发
    pub skip_if_running: bool,

    /// 备注
    pub remark: Option<String>,
}

impl ScheduleTaskDefinition {
    /// 创建定时任务定义
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cron: None,
            supports_parameter: false,
            skip_if_running: false,
            remark: None,
        }
    }

    /// 设置默认 Cron 表达式
    pub fn with_cron(mut self, cron: impl Into<String>) -> Self {
        self.cron = Some(cron.into());
        self
    }

    /// 设置支持参数
    pub fn with_parameter_support(mut self) -> Self {
        self.supports_parameter = true;
        self
    }

    /// 设置上次执行未完成时跳过本次触发
    pub fn with_skip_if_running(mut self) -> Self {
        self.skip_if_running = true;
        self
    }

    /// 设置备注信息
    pub fn with_remark(mut self, remark: impl Into<String>) -> Self {
        self.remark = Some(remark.into());
        self
    }
}

/// 队列任务注册项
#[derive(Clone)]
struct QueueTaskRegistration {
    /// 任务定义
    definition: QueueTaskDefinition,

    /// 任务处理器
    handler: Arc<dyn QueueTaskHandler>,
}

/// 定时任务注册项
#[derive(Clone)]
struct ScheduleTaskRegistration {
    /// 任务定义
    definition: ScheduleTaskDefinition,

    /// 任务处理器
    handler: Arc<dyn ScheduleTaskHandler>,
}

/// 任务注册表
#[derive(Clone, Default)]
pub struct TaskRegistry {
    /// 队列任务注册表
    queue_handlers: BTreeMap<String, QueueTaskRegistration>,

    /// 队列任务别名目标注册表
    queue_alias_targets: BTreeMap<String, String>,

    /// 定时任务注册表
    schedule_handlers: BTreeMap<String, ScheduleTaskRegistration>,

    /// 定时任务别名目标注册表
    schedule_alias_targets: BTreeMap<String, String>,
}

impl TaskRegistry {
    /// 创建任务注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册队列任务
    pub fn register_queue<T>(&mut self, name: impl Into<String>, handler: T) -> Result<(), String>
    where
        T: QueueTaskHandler + 'static,
    {
        self.register_queue_with_definition(QueueTaskDefinition::new(name), handler)
    }

    /// 注册带定义的队列任务
    pub fn register_queue_with_definition<T>(
        &mut self,
        definition: QueueTaskDefinition,
        handler: T,
    ) -> Result<(), String>
    where
        T: QueueTaskHandler + 'static,
    {
        let name = definition.name.clone();
        if self.queue_handlers.contains_key(&name) {
            return Err(format!("队列任务 {name:?} 重复注册"));
        }
        if self.queue_alias_targets.contains_key(&name) {
            return Err(format!("队列任务 {name:?} 与已注册别名同名"));
        }

        self.queue_handlers.insert(
            name,
            QueueTaskRegistration {
                definition,
                handler: Arc::new(handler),
            },
        );

        Ok(())
    }

    /// 注册队列任务别名
    pub fn register_queue_alias(
        &mut self,
        alias: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<(), String> {
        let alias = alias.into();
        if self.queue_handlers.contains_key(&alias) {
            return Err(format!("队列任务别名 {alias:?} 与已注册任务同名"));
        }
        if self.queue_alias_targets.contains_key(&alias) {
            return Err(format!("队列任务别名 {alias:?} 重复注册"));
        }

        self.queue_alias_targets.insert(alias, target.into());

        Ok(())
    }

    /// 注册定时任务
    pub fn register_schedule<T>(
        &mut self,
        name: impl Into<String>,
        handler: T,
    ) -> Result<(), String>
    where
        T: ScheduleTaskHandler + 'static,
    {
        self.register_schedule_with_definition(ScheduleTaskDefinition::new(name), handler)
    }

    /// 注册带定义的定时任务
    pub fn register_schedule_with_definition<T>(
        &mut self,
        definition: ScheduleTaskDefinition,
        handler: T,
    ) -> Result<(), String>
    where
        T: ScheduleTaskHandler + 'static,
    {
        let name = definition.name.clone();
        if self.schedule_handlers.contains_key(&name) {
            return Err(format!("定时任务 {name:?} 重复注册"));
        }
        if self.schedule_alias_targets.contains_key(&name) {
            return Err(format!("定时任务 {name:?} 与已注册别名同名"));
        }

        self.schedule_handlers.insert(
            name,
            ScheduleTaskRegistration {
                definition,
                handler: Arc::new(handler),
            },
        );

        Ok(())
    }

    /// 注册定时任务别名
    pub fn register_schedule_alias(
        &mut self,
        alias: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<(), String> {
        let alias = alias.into();
        if self.schedule_handlers.contains_key(&alias) {
            return Err(format!("定时任务别名 {alias:?} 与已注册任务同名"));
        }
        if self.schedule_alias_targets.contains_key(&alias) {
            return Err(format!("定时任务别名 {alias:?} 重复注册"));
        }

        self.schedule_alias_targets.insert(alias, target.into());

        Ok(())
    }

    /// 校验全部别名并解析到最终正式任务名
    pub fn resolve_aliases(&mut self) -> Result<(), String> {
        self.queue_alias_targets =
            resolve_alias_targets(&self.queue_alias_targets, &self.queue_handlers, "队列任务")?;
        self.schedule_alias_targets = resolve_alias_targets(
            &self.schedule_alias_targets,
            &self.schedule_handlers,
            "定时任务",
        )?;

        Ok(())
    }

    /// 获取队列任务处理器
    pub fn get_queue_handler(&self, name: &str) -> Option<Arc<dyn QueueTaskHandler>> {
        self.queue_handlers
            .get(name)
            .map(|item| item.handler.clone())
            .or_else(|| {
                resolve_alias_target(&self.queue_alias_targets, &self.queue_handlers, name)
                    .ok()
                    .and_then(|target| self.queue_handlers.get(&target))
                    .map(|item| item.handler.clone())
            })
    }

    /// 获取队列任务定义
    /// 供运行器读取默认并发信号量等元数据
    pub fn get_queue_definition(&self, name: &str) -> Option<QueueTaskDefinition> {
        self.queue_handlers
            .get(name)
            .map(|item| item.definition.clone())
    }

    /// 获取定时任务处理器
    pub fn get_schedule_handler(&self, name: &str) -> Option<Arc<dyn ScheduleTaskHandler>> {
        self.schedule_handlers
            .get(name)
            .map(|item| item.handler.clone())
            .or_else(|| {
                resolve_alias_target(&self.schedule_alias_targets, &self.schedule_handlers, name)
                    .ok()
                    .and_then(|target| self.schedule_handlers.get(&target))
                    .map(|item| item.handler.clone())
            })
    }

    /// 获取定时任务定义
    pub fn get_schedule_definition(&self, name: &str) -> Option<ScheduleTaskDefinition> {
        self.schedule_handlers
            .get(name)
            .map(|item| item.definition.clone())
    }

    /// 获取全部已注册队列任务名称
    pub fn queue_names(&self) -> Vec<String> {
        self.queue_handlers.keys().cloned().collect()
    }

    /// 获取全部已注册定时任务名称
    pub fn schedule_names(&self) -> Vec<String> {
        self.schedule_handlers.keys().cloned().collect()
    }

    /// 获取全部队列任务定义
    pub fn queue_definitions(&self) -> Vec<QueueTaskDefinition> {
        self.queue_handlers
            .values()
            .map(|item| item.definition.clone())
            .collect()
    }

    /// 获取全部定时任务定义
    pub fn schedule_definitions(&self) -> Vec<ScheduleTaskDefinition> {
        self.schedule_handlers
            .values()
            .map(|item| item.definition.clone())
            .collect()
    }

    /// 获取全部已注册任务定义
    pub fn registered_tasks(&self) -> Vec<RegisteredTaskDefinition> {
        let mut tasks =
            Vec::with_capacity(self.queue_handlers.len() + self.schedule_handlers.len());

        for item in self.queue_handlers.values() {
            tasks.push(RegisteredTaskDefinition {
                kind: RegisteredTaskKind::Queue,
                name: item.definition.name.clone(),
                supports_parameter: false,
            });
        }

        for item in self.schedule_handlers.values() {
            tasks.push(RegisteredTaskDefinition {
                kind: RegisteredTaskKind::Schedule,
                name: item.definition.name.clone(),
                supports_parameter: item.definition.supports_parameter,
            });
        }

        tasks
    }
}

/// 校验并扁平化同类任务的全部别名目标
fn resolve_alias_targets<T>(
    aliases: &BTreeMap<String, String>,
    handlers: &BTreeMap<String, T>,
    task_kind: &str,
) -> Result<BTreeMap<String, String>, String> {
    aliases
        .keys()
        .map(|alias| {
            resolve_alias_target(aliases, handlers, alias)
                .map(|target| (alias.clone(), target))
                .map_err(|error| format!("{task_kind}{error}"))
        })
        .collect()
}

/// 解析单个别名指向的最终正式任务名
fn resolve_alias_target<T>(
    aliases: &BTreeMap<String, String>,
    handlers: &BTreeMap<String, T>,
    alias: &str,
) -> Result<String, String> {
    let mut path = Vec::new();
    let mut current = alias.to_string();

    loop {
        if handlers.contains_key(&current) {
            return Ok(current);
        }

        if let Some(cycle_start) = path.iter().position(|item| item == &current) {
            let mut cycle = path[cycle_start..].to_vec();
            cycle.push(current);
            let cycle = cycle
                .iter()
                .map(|item| format!("{item:?}"))
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(format!("别名存在循环引用: {cycle}"));
        }

        path.push(current.clone());
        let Some(target) = aliases.get(&current) else {
            return Err(format!("别名 {alias:?} 指向不存在的目标 {current:?}"));
        };
        current.clone_from(target);
    }
}
