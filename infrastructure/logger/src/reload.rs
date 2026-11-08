use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use config::{Config, ConfigError, File, FileFormat, Map, Source, Value, ValueKind};
use notify::{Event, EventKind, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{LoggerFilterHandle, LoggerSettings, log_filter_environment, log_filter_source};

/// 只保留过滤配置的启动快照，不在监听器中保存数据库等连接配置。
#[derive(Clone, Debug)]
pub struct LogFilterReloadSource {
    files: Vec<ReloadFile>,
    environment: SnapshotSource,
}

#[derive(Clone, Debug)]
struct ReloadFile {
    path: PathBuf,
    snapshot: SnapshotSource,
}

#[derive(Clone, Debug, Default)]
struct SnapshotSource(Map<String, Value>);

impl Source for SnapshotSource {
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }
    fn collect(&self) -> Result<Map<String, Value>, ConfigError> {
        Ok(self.0.clone())
    }
}

impl LoggerSettings {
    pub fn set_filter_reload_source(&mut self, source: LogFilterReloadSource) {
        self.reload_source = Some(source);
    }
}

/// 按宿主约定加载 TOML，保存相同读取结果中的过滤配置供后续文件重载。
pub fn load_host_configuration(
    config_path: &str,
    environment: Option<&str>,
) -> Result<(Config, LogFilterReloadSource), ConfigError> {
    let mut paths = vec![(config_path.to_owned(), true)];
    if let Some(environment) = environment {
        paths.push((format!("{config_path}.{environment}"), false));
    }
    let mut builder = Config::builder();
    let mut files = Vec::new();
    for (name, required) in paths {
        let path = toml_path(&name).map_err(ConfigError::Foreign)?;
        let source = log_filter_source(
            File::from(path.as_path())
                .format(FileFormat::Toml)
                .required(required),
        );
        let snapshot = SnapshotSource(source.collect()?);
        files.push(ReloadFile {
            path,
            snapshot: filter_snapshot(&snapshot),
        });
        builder = builder.add_source(snapshot);
    }
    let environment = SnapshotSource(log_filter_environment().collect()?);
    let reload = LogFilterReloadSource {
        files,
        environment: filter_snapshot(&environment),
    };
    Ok((builder.add_source(environment).build()?, reload))
}

fn toml_path(name: &str) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let path = if Path::new(name)
        .extension()
        .is_some_and(|extension| extension == "toml")
    {
        PathBuf::from(name)
    } else {
        PathBuf::from(format!("{name}.toml"))
    };
    let absolute = std::path::absolute(path)?;
    let parent = absolute
        .parent()
        .ok_or("配置文件缺少父目录")?
        .canonicalize()?;
    Ok(parent.join(absolute.file_name().ok_or("配置文件缺少名称")?))
}

fn filter_snapshot(source: &SnapshotSource) -> SnapshotSource {
    let mut root = Map::new();
    if let Some(mut logger) = source.0.get("logger").cloned() {
        if let ValueKind::Table(fields) = &mut logger.kind {
            fields.retain(|name, _| name == "log_level" || name == "providers");
            if let Some(Value {
                kind: ValueKind::Table(providers),
                ..
            }) = fields.get_mut("providers")
            {
                for provider in providers.values_mut() {
                    if let ValueKind::Table(fields) = &mut provider.kind {
                        fields.retain(|name, _| name == "log_level");
                    }
                }
            }
        }
        root.insert("logger".into(), logger);
    }
    SnapshotSource(root)
}

enum ReloadOutcome {
    Loaded(SnapshotSource),
    ParseFailed,
    ReadFailed,
}

fn read_changed_file(path: &Path) -> ReloadOutcome {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ReloadOutcome::Loaded(SnapshotSource::default());
        }
        Err(_) => return ReloadOutcome::ReadFailed,
    };
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    let text = String::from_utf8_lossy(bytes);
    match log_filter_source(File::from_str(&text, FileFormat::Toml)).collect() {
        Ok(values) => ReloadOutcome::Loaded(filter_snapshot(&SnapshotSource(values))),
        Err(_) => ReloadOutcome::ParseFailed,
    }
}

impl LogFilterReloadSource {
    fn settings(&self) -> Result<LoggerSettings, ConfigError> {
        #[derive(Deserialize)]
        struct FilterSettings {
            #[serde(default)]
            logger: LoggerSettings,
        }
        let mut builder = Config::builder();
        for file in &self.files {
            builder = builder.add_source(file.snapshot.clone());
        }
        Ok(builder
            .add_source(self.environment.clone())
            .build()?
            .try_deserialize::<FilterSettings>()?
            .logger)
    }
}

enum FileWatcher {
    Native(RecommendedWatcher),
    Poll(PollWatcher),
}

impl FileWatcher {
    fn watch(&mut self, path: &Path) -> notify::Result<()> {
        match self {
            Self::Native(watcher) => watcher.watch(path, RecursiveMode::NonRecursive),
            Self::Poll(watcher) => watcher.watch(path, RecursiveMode::NonRecursive),
        }
    }
}

pub(super) struct FilterWatcher {
    watcher: Option<FileWatcher>,
    task: Option<JoinHandle<()>>,
}

impl FilterWatcher {
    pub(super) fn start(
        mut source: LogFilterReloadSource,
        handle: LoggerFilterHandle,
    ) -> Result<Self, String> {
        let paths: Vec<_> = source.files.iter().map(|file| file.path.clone()).collect();
        let all = (1_u64 << paths.len()) - 1;
        const WATCH_ERROR: u64 = 1 << 63;
        let pending = Arc::new(AtomicU64::new(all));
        let signal = Arc::clone(&pending);
        let (tx, mut rx) = mpsc::channel(1);
        let initial = tx.clone();
        let callback = move |result: notify::Result<Event>| {
            let flags = match result {
                Ok(event) if matches!(event.kind, EventKind::Access(_)) => 0,
                Ok(event) if event.need_rescan() => all,
                Ok(event) => paths.iter().enumerate().fold(0, |flags, (index, path)| {
                    flags
                        | if event.paths.contains(path) {
                            1 << index
                        } else {
                            0
                        }
                }),
                Err(_) => all | WATCH_ERROR,
            };
            if flags != 0 {
                signal.fetch_or(flags, Ordering::Release);
                let _ = tx.try_send(());
            }
        };
        let poll = std::env::var("DOTNET_USE_POLLING_FILE_WATCHER")
            .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let mut watcher = if poll {
            FileWatcher::Poll(
                PollWatcher::new(
                    callback,
                    notify::Config::default().with_poll_interval(Duration::from_secs(4)),
                )
                .map_err(|error| format!("日志配置轮询监听初始化失败: {error}"))?,
            )
        } else {
            FileWatcher::Native(
                notify::recommended_watcher(callback)
                    .map_err(|error| format!("日志配置文件监听初始化失败: {error}"))?,
            )
        };
        let mut parents = HashSet::new();
        for file in &source.files {
            if let Some(parent) = file.path.parent()
                && parents.insert(parent.to_owned())
            {
                watcher
                    .watch(parent)
                    .map_err(|error| format!("日志配置目录监听失败: {error}"))?;
            }
        }
        // 补齐首次读取到监听注册之间的文件变化，环境来源保持启动快照。
        let _ = initial.try_send(());
        let task = tokio::spawn(async move {
            while rx.recv().await.is_some() {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let flags = pending.swap(0, Ordering::AcqRel);
                if flags & WATCH_ERROR != 0 {
                    report_reload_failure();
                }
                for index in 0..source.files.len() {
                    if flags & (1 << index) == 0 {
                        continue;
                    }
                    let path = source.files[index].path.clone();
                    let loaded =
                        tokio::task::spawn_blocking(move || read_changed_file(&path)).await;
                    match loaded {
                        Ok(ReloadOutcome::Loaded(snapshot)) => {
                            source.files[index].snapshot = snapshot
                        }
                        Ok(ReloadOutcome::ParseFailed) => {
                            // 源解析失败会清空该文件来源，但不发布重载通知。
                            source.files[index].snapshot = SnapshotSource::default();
                            report_reload_failure();
                            continue;
                        }
                        Ok(ReloadOutcome::ReadFailed) | Err(_) => {
                            report_reload_failure();
                            continue;
                        }
                    }
                    if source
                        .settings()
                        .map_err(|_| ())
                        .and_then(|settings| handle.reload(&settings).map_err(|_| ()))
                        .is_err()
                    {
                        report_reload_failure();
                    }
                }
            }
        });
        Ok(Self {
            watcher: Some(watcher),
            task: Some(task),
        })
    }

    pub(super) async fn shutdown(mut self) {
        self.watcher.take();
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for FilterWatcher {
    fn drop(&mut self) {
        self.watcher.take();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn report_reload_failure() {
    // 不回写日志提供者，不打印配置正文或连接信息。
    eprintln!("日志过滤配置重载失败，请检查配置文件与文件监听状态");
}
