use std::collections::{HashMap, hash_map::Entry};

use common::{app_error::AppError, string_helper::ordinal_ignore_case_key};

/// 提示词参数的比较方式由调用方决定，不改变模板和参数值。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LlmParameterComparison {
    #[default]
    Ordinal,
    OrdinalIgnoreCase,
}

/// 带比较方式的模板参数字典，null 与缺失键保持区别。
#[derive(Clone, Debug, Default)]
pub struct LlmParameters {
    values: HashMap<String, Option<String>>,
    comparison: LlmParameterComparison,
}

impl LlmParameters {
    pub fn new(comparison: LlmParameterComparison) -> Self {
        Self {
            values: HashMap::new(),
            comparison,
        }
    }

    /// 转换现有字典时拒绝比较后重名的键，与源字典构造行为一致。
    pub fn from_map(
        values: HashMap<String, Option<String>>,
        comparison: LlmParameterComparison,
    ) -> Result<Self, AppError> {
        let mut parameters = Self::new(comparison);
        for (key, value) in values {
            let key = parameters.key(&key);
            match parameters.values.entry(key) {
                Entry::Vacant(entry) => {
                    entry.insert(value);
                }
                Entry::Occupied(_) => {
                    return Err(AppError::internal(
                        "LLM parameter names collide under the selected comparison",
                    ));
                }
            }
        }
        Ok(parameters)
    }

    /// 按所选比较方式赋值，存在等价键时替换原值。
    pub fn insert(
        &mut self,
        key: impl AsRef<str>,
        value: Option<String>,
    ) -> Option<Option<String>> {
        self.values.insert(self.key(key.as_ref()), value)
    }

    pub fn get(&self, key: &str) -> Option<&Option<String>> {
        self.values.get(&self.key(key))
    }

    pub fn comparison(&self) -> LlmParameterComparison {
        self.comparison
    }

    fn key(&self, key: &str) -> String {
        match self.comparison {
            LlmParameterComparison::Ordinal => key.to_owned(),
            LlmParameterComparison::OrdinalIgnoreCase => ordinal_ignore_case_key(key),
        }
    }
}

impl From<HashMap<String, Option<String>>> for LlmParameters {
    fn from(values: HashMap<String, Option<String>>) -> Self {
        Self {
            values,
            comparison: LlmParameterComparison::Ordinal,
        }
    }
}
