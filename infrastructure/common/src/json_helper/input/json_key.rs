//! 区分真实对象属性与 serde_json 任意精度数字的内部 MapAccess 协议。
use serde::{
    Deserialize, Deserializer,
    de::{self, DeserializeSeed, Visitor},
};
use std::fmt;

pub(super) enum Key {
    Property(String),
    Number,
    Object,
}

pub(super) struct KeySeed {
    pub read_property: bool,
}

impl<'de> DeserializeSeed<'de> for KeySeed {
    type Value = Key;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Key, D::Error> {
        // 真正的 JSON 属性名保留 newtype 边界；NumberFieldDeserializer 则直接提供静态标记。
        deserializer.deserialize_newtype_struct("Common.Json.Property", self)
    }
}

impl<'de> Visitor<'de> for KeySeed {
    type Value = Key;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON 属性名")
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<Key, D::Error> {
        if self.read_property {
            String::deserialize(deserializer).map(Key::Property)
        } else {
            Ok(Key::Object)
        }
    }
    fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Key, E> {
        if value == "$serde_json::private::Number" {
            Ok(Key::Number)
        } else {
            self.visit_str(value)
        }
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Key, E> {
        self.visit_string(value.to_owned())
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Key, E> {
        Ok(if self.read_property {
            Key::Property(value)
        } else {
            Key::Object
        })
    }
}
