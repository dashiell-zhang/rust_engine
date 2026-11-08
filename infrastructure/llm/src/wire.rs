//! 提供商 DTO 的字段匹配；字典和动态 JSON 保留原始键名与值。
use serde::{
    Deserialize, Deserializer,
    de::{
        self, DeserializeOwned, DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor,
    },
};
use std::fmt;
type Error = de::value::Error;

/// 引用类型集合允许 null；只有实际访问元素时才失败，不能提前否定被覆盖或未使用的集合。
pub(crate) fn require_object<T>(value: Option<T>) -> Result<T, crate::LlmError> {
    value.ok_or_else(|| {
        crate::LlmError::InvalidResponse(
            "Object reference not set to an instance of an object.".into(),
        )
    })
}

enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

/// 提供商错误是动态 JSON；普通属性不能被 serde_json::Value 当成内部数字/原文协议。
#[derive(Deserialize)]
#[serde(transparent)]
pub(crate) struct ErrorValue(Value);

impl ErrorValue {
    pub(crate) fn as_str(&self) -> Option<&str> {
        match &self.0 {
            Value::String(value) => Some(value),
            _ => None,
        }
    }

    /// JsonElement.TryGetProperty 按原始大小写查找，并使用最后一个同名属性。
    pub(crate) fn string_property(&self, name: &str) -> Option<&str> {
        let Value::Object(entries) = &self.0 else {
            return None;
        };
        match &entries.iter().rfind(|(key, _)| key == name)?.1 {
            Value::String(value) => Some(value),
            _ => None,
        }
    }
}

struct ValueVisitor;
impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON 值")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("不支持非有限数字"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut fields = Vec::new();
        while let Some(entry) = map.next_entry()? {
            fields.push(entry);
        }
        Ok(Value::Object(fields))
    }
}
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(ValueVisitor)
    }
}

pub(crate) fn from_slice<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_slice::<Wire<T>>(bytes).map(|wire| wire.0)
}
struct Wire<T>(T);
impl<'de, T: DeserializeOwned> Deserialize<'de> for Wire<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(Input(Value::deserialize(deserializer)?))
            .map(Self)
            .map_err(de::Error::custom)
    }
}
struct Input(Value);
impl Input {
    fn map<'de, V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let Value::Object(entries) = self.0 else {
            return Err(de::Error::custom("expected object"));
        };
        // 不提前合并字段；协议 DTO 逐项校验，动态对象保留原始名称与顺序。
        visitor.visit_map(Fields {
            entries: entries.into_iter(),
            pending: None,
        })
    }
}
struct Fields {
    entries: std::vec::IntoIter<(String, Value)>,
    pending: Option<Value>,
}
impl<'de> MapAccess<'de> for Fields {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        match self.entries.next() {
            Some((key, value)) => {
                self.pending = Some(value);
                seed.deserialize(key.into_deserializer()).map(Some)
            }
            None => Ok(None),
        }
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        seed.deserialize(Input(
            self.pending
                .take()
                .ok_or_else(|| <Error as de::Error>::custom("missing value"))?,
        ))
    }
}
struct Items(std::vec::IntoIter<Value>);
impl<'de> SeqAccess<'de> for Items {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.0
            .next()
            .map(|value| seed.deserialize(Input(value)))
            .transpose()
    }
}
impl<'de> Deserializer<'de> for Input {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Null => visitor.visit_unit(),
            Value::Bool(value) => visitor.visit_bool(value),
            Value::Number(value) => serde_json::Value::Number(value)
                .deserialize_any(visitor)
                .map_err(de::Error::custom),
            Value::String(value) => visitor.visit_string(value),
            Value::Array(values) => visitor.visit_seq(Items(values.into_iter())),
            Value::Object(_) => self.map(visitor),
        }
    }
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.map(visitor)
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }
    serde::forward_to_deserialize_any! { bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map enum identifier ignored_any }
}
