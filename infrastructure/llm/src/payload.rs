use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{Value, value::RawValue};

use crate::{LlmError, models::ExtraBody};

/// 基础字段按协议生成，扩展子节点直接写回；两者都保持字段插入顺序。
#[derive(Default, Serialize)]
#[serde(transparent)]
pub(crate) struct Payload(IndexMap<String, Field>);

#[derive(Serialize)]
#[serde(untagged)]
enum Field {
    Generated(Value),
    Raw(Box<RawValue>),
}

impl Payload {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn insert(&mut self, key: String, value: Value) {
        self.0.insert(key, Field::Generated(value));
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub(crate) fn merge_extra(&mut self, extra: &ExtraBody) -> Result<(), LlmError> {
        for (key, value) in extra {
            if key.trim().is_empty() {
                continue;
            }
            if self.contains_key(key) {
                return Err(LlmError::InvalidRequest(format!(
                    "ExtraBody conflicts with base payload field: {key}"
                )));
            }
            self.0.insert(key.clone(), Field::Raw(value.clone()));
        }
        Ok(())
    }
}
