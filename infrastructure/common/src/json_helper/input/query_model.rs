use super::{Error, Input, InputError, Source, Value};
use serde::{
    Deserializer,
    de::{self, DeserializeOwned, Visitor},
};

/// Query 模型仅在所有字段绑定与字段验证通过后构造，不将部分成功值交给业务。
pub trait QueryModel: Sized {
    fn bind_query(fields: &[(String, String)]) -> Result<Self, Vec<InputError>>;
}

/// 供派生宏绑定单个字段；只复制该字段及别名的值，未知参数不参与分配。
#[doc(hidden)]
pub fn query_field<T: DeserializeOwned>(fields: &[(String, String)]) -> Result<T, InputError> {
    T::deserialize(FieldInput(fields))
}

struct FieldInput<'a>(&'a [(String, String)]);
impl<'de> Deserializer<'de> for FieldInput<'_> {
    type Error = Error;

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        names: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        let fields = self
            .0
            .iter()
            .filter(|(key, _)| {
                names
                    .iter()
                    .any(|name| crate::string_helper::eq_ordinal_ignore_case(name, key))
            })
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect();
        Input {
            value: Value::Object(fields),
            source: Source::Query,
        }
        .map(names, visitor)
    }

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Error> {
        Err(de::Error::custom("Query 字段绑定需要具名字段结构体"))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map enum identifier ignored_any
    }
}
