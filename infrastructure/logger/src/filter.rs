use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use crate::{LoggerProvider, LoggerSettings, level};

impl LoggerSettings {
    /// 在创建日志后台任务前校验级别和类别规则，不能静默回退默认规则。
    pub fn validate_log_filter(&self, provider: LoggerProvider) -> Result<(), String> {
        // 源配置加载时也解析尚未注册的提供者的级别，但不匹配其类别。
        for levels in std::iter::once(&self.log_level)
            .chain(self.providers.values().map(|settings| &settings.log_level))
        {
            for (target, text) in levels {
                level::parse(text).map_err(|()| format!("日志级别配置无效: target={target}"))?;
            }
        }
        ProviderFilter::new(self, LoggerProvider::Console)?;
        ProviderFilter::new(self, provider).map(|_| ())
    }
}

pub(super) struct ProviderFilter {
    global: CategoryFilter,
    specific: CategoryFilter,
}

impl ProviderFilter {
    pub(super) fn new(settings: &LoggerSettings, provider: LoggerProvider) -> Result<Self, String> {
        let mut sources: Vec<_> = settings
            .providers
            .iter()
            .filter(|(name, _)| provider.names().contains(&name.as_str()))
            .collect();
        // 提供者配置节按正序枚举，别名和完整类型名不另设优先级。
        sources.sort_by(|(left, _), (right, _)| {
            configuration_key_cmp(&ordinal_key(left), &ordinal_key(right))
        });
        Ok(Self {
            global: CategoryFilter::new(std::iter::once(&settings.log_level))?,
            specific: CategoryFilter::new(
                sources.into_iter().map(|(_, settings)| &settings.log_level),
            )?,
        })
    }

    pub(super) fn minimum(&self, category: &str) -> i32 {
        // 提供者默认规则也优先于更长的全局类别规则。
        self.specific
            .minimum(category)
            .or_else(|| self.global.minimum(category))
            .unwrap_or(2)
    }
}

struct CategoryFilter {
    default_level: Option<i32>,
    rules: Vec<CategoryRule>,
}

struct CategoryRule {
    key: String,
    prefix: String,
    suffix: String,
    length: usize,
    level: i32,
    source_order: usize,
}

impl CategoryRule {
    fn matches(&self, category: &str) -> bool {
        // 动态过滤按事件检查，不为每条日志分配大写类别副本。
        let mut prefix = ordinal_chars(category);
        if !self.prefix.chars().all(|ch| prefix.next() == Some(ch)) {
            return false;
        }
        let mut suffix = ordinal_chars(category).rev();
        self.suffix
            .chars()
            .rev()
            .all(|ch| suffix.next() == Some(ch))
    }
}

impl CategoryFilter {
    fn new<'a>(
        sources: impl IntoIterator<Item = &'a HashMap<String, String>>,
    ) -> Result<Self, String> {
        let mut filter = Self {
            default_level: None,
            rules: Vec::new(),
        };
        for (source_order, levels) in sources.into_iter().enumerate() {
            let mut names = HashSet::new();
            for (target, text) in levels {
                let key = ordinal_key(target);
                if !names.insert(key.clone()) {
                    return Err(format!("日志类别配置重复（不区分大小写）: target={target}"));
                }
                let Some(level) = level::parse(text)
                    .map_err(|()| format!("日志级别配置无效: target={target}"))?
                else {
                    continue;
                };
                if key == "DEFAULT" {
                    filter.default_level = Some(level);
                    continue;
                }
                let (prefix, suffix) = key.split_once('*').unwrap_or((&key, ""));
                if suffix.contains('*') {
                    return Err(format!("日志类别最多允许一个通配符: target={target}"));
                }
                filter.rules.push(CategoryRule {
                    prefix: prefix.to_owned(),
                    suffix: suffix.to_owned(),
                    key,
                    length: target.encode_utf16().count(),
                    level,
                    source_order,
                });
            }
        }
        // 源配置按子键升序入栈、逆序枚举，等长规则取最后匹配项。
        // 这里按优先级排列后取第一个，避免依赖 HashMap 的随机枚举顺序。
        filter.rules.sort_by(|left, right| {
            right
                .length
                .cmp(&left.length)
                .then_with(|| right.source_order.cmp(&left.source_order))
                .then_with(|| configuration_key_cmp(&left.key, &right.key))
        });
        Ok(filter)
    }

    fn minimum(&self, category: &str) -> Option<i32> {
        self.rules
            .iter()
            // 源实现分别检查前缀和后缀，允许两者在类别中重叠。
            .find(|rule| rule.matches(category))
            .map(|rule| rule.level)
            .or(self.default_level)
    }
}

// 与 common::string_helper 的 OrdinalIgnoreCase 使用相同的一对一映射。
// 提供者基础契约不依赖包含图像、表格及业务工具的 common crate。
pub(super) fn ordinal_key(value: &str) -> String {
    ordinal_chars(value).collect()
}

fn ordinal_chars(value: &str) -> impl DoubleEndedIterator<Item = char> + '_ {
    let mapper = icu_casemap::CaseMapper::new();
    value.chars().map(move |ch| match ch {
        '\u{0131}' | '\u{017f}' => ch,
        _ => mapper.simple_uppercase(ch),
    })
}

fn configuration_key_cmp(left: &str, right: &str) -> Ordering {
    // GetChildren 对每一级子键排序；空层级也参与，不能先折叠连续冒号。
    let mut left_parts = left.split(':');
    let mut right_parts = right.split(':');
    loop {
        let order = match (left_parts.next(), right_parts.next()) {
            (Some(left), Some(right)) => {
                match (left.trim().parse::<i32>(), right.trim().parse::<i32>()) {
                    (Ok(left), Ok(right)) => left.cmp(&right),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => left.encode_utf16().cmp(right.encode_utf16()),
                }
            }
            (Some(_), None) => Ordering::Greater,
            (None, Some(_)) => Ordering::Less,
            (None, None) => return Ordering::Equal,
        };
        if order != Ordering::Equal {
            return order;
        }
    }
}
