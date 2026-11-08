use config::{ConfigError, Environment, Map, Source, Value, ValueKind};

/// 沿用宿主 APP/__ 环境配置，同时保留日志提供者名称和类别的字面点号。
pub fn log_filter_environment() -> impl Source + Clone + Send + Sync {
    log_filter_source(LogFilterEnvironment)
}

#[derive(Clone, Debug)]
struct LogFilterEnvironment;

impl Source for LogFilterEnvironment {
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<Map<String, Value>, ConfigError> {
        let mut ordinary = Map::new();
        let mut logging = Vec::new();
        for (key, value) in std::env::vars_os() {
            let Ok(key) = key.into_string() else { continue };
            if !key.to_ascii_lowercase().starts_with("app__") {
                continue;
            }
            let value = value.into_string().map_err(|_| {
                ConfigError::Message("APP 环境变量包含无法解析的 Unicode 值".into())
            })?;
            if let Some(path) = log_filter_path(&key) {
                logging.push((path, value));
            } else {
                ordinary.insert(key, value);
            }
        }
        let mut collected = Value::from(Map::<String, Value>::new());
        Environment::with_prefix("APP")
            .separator("__")
            .source(Some(ordinary))
            .collect_to(&mut collected)?;
        let ValueKind::Table(mut root) = collected.kind else {
            return Err(ConfigError::Message("宿主配置根必须为对象".into()));
        };
        let origin = String::from("the environment");
        for (path, value) in logging {
            let mut table = &mut root;
            for segment in &path[..path.len() - 1] {
                let entry = table
                    .entry(segment.clone())
                    .or_insert_with(|| Value::from(Map::<String, Value>::new()));
                let ValueKind::Table(next) = &mut entry.kind else {
                    return Err(ConfigError::Message("日志环境配置的对象与标量冲突".into()));
                };
                table = next;
            }
            table.insert(
                // 环境来源按枚举顺序覆盖同名键；文件来源仍拒绝重复类别。
                crate::filter::ordinal_key(path.last().expect("日志路径非空")),
                Value::new(Some(&origin), value),
            );
        }
        Ok(root)
    }
}

fn log_filter_path(name: &str) -> Option<Vec<String>> {
    let mut parts = name.splitn(4, "__");
    if !parts.next()?.eq_ignore_ascii_case("APP") || !parts.next()?.eq_ignore_ascii_case("LOGGER") {
        return None;
    }
    let kind = parts.next()?;
    let rest = parts.next()?;
    let mut path = vec!["logger".into()];
    if kind.eq_ignore_ascii_case("LOG_LEVEL") {
        path.extend(["log_level".into(), rest.replace("__", ":")]);
    } else if kind.eq_ignore_ascii_case("PROVIDERS") {
        let mut provider = rest.splitn(3, "__");
        let name = provider.next()?;
        if !provider.next()?.eq_ignore_ascii_case("LOG_LEVEL") {
            return None;
        }
        path.extend([
            "providers".into(),
            name.into(),
            "log_level".into(),
            provider.next()?.replace("__", ":"),
        ]);
    } else {
        return None;
    }
    Some(path)
}

/// 在每一层配置合并前统一日志类别键，使环境覆盖不受类别大小写影响。
pub fn log_filter_source<S>(source: S) -> impl Source + Clone + Send + Sync
where
    S: Source + Clone + Send + Sync + 'static,
{
    LogFilterSource(source)
}

#[derive(Clone, Debug)]
struct LogFilterSource<S>(S);

impl<S> Source for LogFilterSource<S>
where
    S: Source + Clone + Send + Sync + 'static,
{
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<Map<String, Value>, ConfigError> {
        let mut collected = Value::from(Map::<String, Value>::new());
        self.0.collect_to(&mut collected)?;
        let ValueKind::Table(mut root) = collected.kind else {
            return Err(ConfigError::Message("宿主配置根必须为对象".into()));
        };
        if let Some(Value {
            kind: ValueKind::Table(logger),
            ..
        }) = root.get_mut("logger")
        {
            normalize_levels(logger)?;
            if let Some(Value {
                kind: ValueKind::Table(providers),
                ..
            }) = logger.get_mut("providers")
            {
                for settings in providers.values_mut() {
                    if let ValueKind::Table(settings) = &mut settings.kind {
                        normalize_levels(settings)?;
                    }
                }
            }
        }
        Ok(root)
    }
}

fn normalize_levels(settings: &mut Map<String, Value>) -> Result<(), ConfigError> {
    if let Some(Value {
        kind: ValueKind::Table(levels),
        ..
    }) = settings.get_mut("log_level")
    {
        for (name, value) in std::mem::take(levels) {
            let key = crate::filter::ordinal_key(&name);
            if levels.insert(key, value).is_some() {
                return Err(ConfigError::Message(format!(
                    "同一配置源的日志类别重复（不区分大小写）: target={name}"
                )));
            }
        }
    }
    Ok(())
}
