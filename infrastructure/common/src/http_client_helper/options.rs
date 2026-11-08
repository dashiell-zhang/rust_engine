use std::{any::Any, collections::HashMap, sync::Arc};

/// 只在进程内传递的请求选项；名称区分大小写，值不会序列化成 HTTP 请求头。
#[derive(Clone, Default)]
pub struct RequestOptions {
    values: HashMap<String, Arc<dyn Any + Send + Sync>>,
}

impl RequestOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// 对应扩展方法的 TryAdd，已有名称保持原值。
    pub fn try_add<T: Any + Send + Sync>(&mut self, name: impl Into<String>, value: T) -> bool {
        match self.values.entry(name.into()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Arc::new(value));
                true
            }
            std::collections::hash_map::Entry::Occupied(_) => false,
        }
    }

    /// 处理器需要调整选项时显式覆盖。
    pub fn set<T: Any + Send + Sync>(&mut self, name: impl Into<String>, value: T) {
        self.values.insert(name.into(), Arc::new(value));
    }

    pub fn get<T: Any>(&self, name: &str) -> Option<&T> {
        self.values.get(name)?.downcast_ref()
    }

    pub fn contains_key(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.values.remove(name).is_some()
    }

    pub fn extend_missing(&mut self, options: &Self) {
        for (name, value) in &options.values {
            self.values
                .entry(name.clone())
                .or_insert_with(|| value.clone());
        }
    }
}
