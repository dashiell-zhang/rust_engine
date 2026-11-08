//! 按模型声明绑定字段；WebAPI 与内部 JSON 共用遍历，不修改字典键或业务内容。

mod json_key;
mod json_model;
mod json_reader;
pub use json_model::JsonObject;
pub use json_reader::{JsonReadError, read_json_model};
mod query_date_time;
pub use json_model::{JsonField, JsonFieldKind, JsonModel, JsonScalar, JsonScalarField};
pub use source_generator_core::{JsonModel, ProtocolModel};
mod query_model;
mod query_number;
pub use query_model::{QueryModel, query_field};
pub use source_generator_core::QueryModel;

use query_number::QueryNumber;

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{
        self, DeserializeOwned, DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor,
    },
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt,
    rc::Rc,
};

/// 绑定失败原因；HTTP 层可按类型生成响应，不反向解析错误文案。
#[derive(Debug)]
pub enum InputErrorKind {
    /// 模型字段声明的验证提示，格式转换成功后统一汇总。
    Validation,
    /// 模型没有默认值且请求缺少该字段；不推断缺少字段的目标类型。
    MissingField(&'static str),
    /// WebAPI 非可空字符串经空白转换后没有值。
    RequiredString,
    /// 已成功绑定的字段不符合声明的数值范围。
    Range { minimum: i64, maximum: i64 },
    /// Query 标量的格式、范围或非可空空值错误；保留未裁剪的请求值。
    InvalidQueryScalar {
        attempted_value: String,
        is_empty: bool,
    },
    /// 其他格式、范围或自定义转换错误。
    Other,
}

/// 保留模型字段路径的反序列化错误。
#[derive(Debug)]
pub struct InputError {
    kind: InputErrorKind,
    path: Vec<String>,
    message: String,
}

impl InputError {
    /// 字段验证由模型声明，HTTP 层负责本地化和属性显示名称。
    pub fn range(field: &str, minimum: i64, maximum: i64) -> Self {
        Self {
            kind: InputErrorKind::Range { minimum, maximum },
            path: vec![field.to_owned()],
            message: format!("值必须在 {minimum} 到 {maximum} 之间"),
        }
    }
    fn invalid_query_scalar(value: String, message: &str) -> Self {
        Self {
            kind: InputErrorKind::InvalidQueryScalar {
                is_empty: value.trim().is_empty(),
                attempted_value: value,
            },
            path: Vec::new(),
            message: message.to_owned(),
        }
    }
    /// 获取原始错误分类。
    pub fn kind(&self) -> &InputErrorKind {
        &self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// 获取从外层到内层的模型字段名称，不包含字典内容改写。
    pub fn path(&self) -> &[String] {
        &self.path
    }

    fn with_field(mut self, field: String) -> Self {
        self.path.insert(0, field);
        self
    }
}

impl fmt::Display for InputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for field in &self.path {
            write!(formatter, "字段 {field}: ")?;
        }
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for InputError {}

impl de::Error for InputError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self {
            kind: InputErrorKind::Other,
            path: Vec::new(),
            message: message.to_string(),
        }
    }

    fn missing_field(field: &'static str) -> Self {
        Self {
            kind: InputErrorKind::MissingField(field),
            path: Vec::new(),
            message: format!("missing field `{field}`"),
        }
    }
}

type Error = InputError;

#[derive(Clone)]
enum Source {
    Json,
    WebProtocol,
    Query,
    Object,
    Clone(ReferenceIds),
}

/// 一次克隆读取内共享引用编号；普通 JSON 与动态文档不登记元数据。
#[derive(Clone, Default)]
struct ReferenceIds(Rc<RefCell<HashSet<String>>>);

impl ReferenceIds {
    fn register(&self, id: &str) -> Result<(), Error> {
        if !self.0.borrow_mut().insert(id.to_owned()) {
            return Err(de::Error::custom("克隆对象引用编号重复"));
        }
        Ok(())
    }
}

/// JSON 对象保留输入顺序及重复键，避免先转成 Map 后丢失最后一次赋值的顺序。
#[derive(Clone)]
enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
    QueryValues(Vec<Value>),
}

impl Value {
    fn into_json(self) -> serde_json::Value {
        match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(value) => serde_json::Value::Bool(value),
            Self::Number(value) => serde_json::Value::Number(value),
            Self::String(value) => serde_json::Value::String(value),
            Self::Array(values) | Self::QueryValues(values) => {
                serde_json::Value::Array(values.into_iter().map(Self::into_json).collect())
            }
            Self::Object(entries) => serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, value.into_json()))
                    .collect(),
            ),
        }
    }
}

/// 动态 JSON 保留原始属性名；不能把普通对象的键再次解释为 Serde 内部数字或原文协议。
pub fn deserialize_json_value<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<serde_json::Value, D::Error> {
    Value::deserialize(deserializer).map(Value::into_json)
}

/// 配置中的动态 JSON 复用同一读取规则，并拒绝根值之后的多余内容。
pub fn parse_json_value(json: &str) -> Result<serde_json::Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let value = deserialize_json_value(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

/// 按声明顺序保留对象项，供调用方区分重复键；子节点不丢失重复属性。
pub type RawJsonObjectEntries = Vec<(String, Box<serde_json::value::RawValue>)>;

/// 配置对象按公共深度上限读取；非对象返回 None，是否允许重复顶层键由用例决定。
pub fn parse_json_object_entries(
    json: &str,
) -> Result<Option<RawJsonObjectEntries>, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let value = Value::deserialize(&mut deserializer)?;
    deserializer.end()?;
    let Value::Object(entries) = value else {
        return Ok(None);
    };
    entries
        .into_iter()
        .map(|(key, value)| serde_json::value::to_raw_value(&value).map(|value| (key, value)))
        .collect::<Result<_, _>>()
        .map(Some)
}

// 原始 JSON 模型可通过 Serde 的 RawValue 协议接收当前子树；不进行字段绑定或数字转换。
impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(value) => value.serialize(serializer),
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(values) | Self::QueryValues(values) => values.serialize(serializer),
            Self::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

struct ValueVisitor {
    depth: usize,
}

impl ValueVisitor {
    fn check_container<E: de::Error>(&self) -> Result<(), E> {
        if self.depth >= super::MAX_DEPTH {
            Err(E::custom(format!(
                "JSON 嵌套深度不能超过 {}",
                super::MAX_DEPTH
            )))
        } else {
            Ok(())
        }
    }
}

impl<'de> DeserializeSeed<'de> for ValueVisitor {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}
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
    fn visit_i128<E: de::Error>(self, v: i128) -> Result<Value, E> {
        serde_json::Number::from_i128(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("JSON 整数超出范围"))
    }
    fn visit_u128<E: de::Error>(self, v: u128) -> Result<Value, E> {
        serde_json::Number::from_u128(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("JSON 整数超出范围"))
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
        self.check_container()?;
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(ValueVisitor {
            depth: self.depth + 1,
        })? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let first = map.next_key_seed(json_key::KeySeed {
            read_property: true,
        });
        // 任意精度数字使用 Serde 的 MapAccess 协议，但不增加 JSON 容器深度。
        if matches!(first, Ok(Some(json_key::Key::Number))) {
            let text = map.next_value::<String>()?;
            return text.parse().map(Value::Number).map_err(de::Error::custom);
        }
        self.check_container()?;
        let mut fields = Vec::new();
        let mut pending = first?;
        while let Some(key) = pending {
            let json_key::Key::Property(key) = key else {
                return Err(de::Error::custom("JSON 属性协议无效"));
            };
            fields.push((
                key,
                map.next_value_seed(ValueVisitor {
                    depth: self.depth + 1,
                })?,
            ));
            pending = map.next_key_seed(json_key::KeySeed {
                read_property: true,
            })?;
        }
        Ok(Value::Object(fields))
    }
}
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        ValueVisitor { depth: 0 }.deserialize(d)
    }
}

/// 内部 JsonHelper 不执行 WebAPI 的必填字符串验证。
struct ObjectInput<T, const CLONE: bool>(T);
impl<'de, T: DeserializeOwned, const CLONE: bool> Deserialize<'de> for ObjectInput<T, CLONE> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        T::deserialize(Input {
            value,
            source: if CLONE {
                Source::Clone(ReferenceIds::default())
            } else {
                Source::Object
            },
        })
        .map(Self)
        .map_err(de::Error::custom)
    }
}

pub(super) fn object<T: DeserializeOwned>(json: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str::<ObjectInput<T, false>>(json).map(|value| value.0)
}

/// Web JSON 协议不运行 HTTP 验证，也不预先合并重复字段。
/// 模型使用 ProtocolModel 时逐次转换已知字段，最后一次有效赋值生效。
pub fn web_protocol<T: DeserializeOwned>(json: &str) -> Result<T, serde_json::Error> {
    let value: Value = serde_json::from_str(json)?;
    T::deserialize(Input {
        value,
        source: Source::WebProtocol,
    })
    .map_err(de::Error::custom)
}

pub(super) fn clone_object<T: DeserializeOwned>(json: &str) -> Result<T, serde_json::Error> {
    serde_json::from_str::<ObjectInput<T, true>>(json).map(|value| value.0)
}

/// 查询字符串的 URL 解码由 HTTP 层负责，此处只处理已解码字段。
pub fn query<T: DeserializeOwned>(
    fields: impl IntoIterator<Item = (String, String)>,
) -> Result<T, Error> {
    let fields = fields
        .into_iter()
        .map(|(key, value)| (key, Value::String(value)))
        .collect();
    T::deserialize(Input {
        value: Value::Object(fields),
        source: Source::Query,
    })
}

/// 显式可空 Query 转换器复用相同的标量解析，避免另走 JSON 的数字规则。
pub(super) fn query_scalar<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, Error> {
    let value = Value::deserialize(value).map_err(de::Error::custom)?;
    T::deserialize(Input {
        value,
        source: Source::Query,
    })
}

/// 可空 HTTP 标量只处理空值，实际非空值继续使用同一目标类型的 JSON 转换
pub(super) fn json_scalar<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, Error> {
    let value = Value::deserialize(value).map_err(de::Error::custom)?;
    T::deserialize(Input {
        value,
        source: Source::Json,
    })
}

const OPTIONAL_QUERY_SCALAR: &str = "$common::optional_query_scalar";
const QUERY_GUID: &str = "$common::query_guid";
const OPTIONAL_QUERY_DATE_TIME: &str = "$common::optional_query_date_time";

/// 显式可空 Query 日期；只有这些声明启用 invariant 日期解析。
pub fn deserialize_optional_query_date_time<'de, D>(
    deserializer: D,
) -> Result<Option<chrono::DateTime<chrono::FixedOffset>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct DateVisitor;
    impl<'de> Visitor<'de> for DateVisitor {
        type Value = Option<chrono::DateTime<chrono::FixedOffset>>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("可空 Query 日期")
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            chrono::DateTime::deserialize(d).map(Some)
        }
        fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            // 非 Query 反序列化保持原来的显式可空 ISO 行为。
            super::optional_scalar(d, false)
        }
    }
    deserializer.deserialize_newtype_struct(OPTIONAL_QUERY_DATE_TIME, DateVisitor)
}

/// 显式 Guid Query 转换；输入错误保留原始值及字段路径。
pub fn deserialize_query_guid<'de, D>(deserializer: D) -> Result<uuid::Uuid, D::Error>
where
    D: Deserializer<'de>,
{
    struct GuidVisitor;
    impl<'de> Visitor<'de> for GuidVisitor {
        type Value = uuid::Uuid;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("Guid 字符串")
        }

        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            let value = String::deserialize(deserializer)?;
            crate::data_helper::guid::parse(&value)
                .ok_or_else(|| de::Error::custom("Guid 格式无效"))
        }
    }
    deserializer.deserialize_newtype_struct(QUERY_GUID, GuidVisitor)
}

/// 通过反序列化协议传递可空声明，避免将 InputError 转为字符串后丢失分类。
pub(super) fn optional_query_scalar<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    struct OptionalScalar<T>(std::marker::PhantomData<T>);
    impl<'de, T: DeserializeOwned> Visitor<'de> for OptionalScalar<T> {
        type Value = Option<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("可空 Query 标量")
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            T::deserialize(deserializer).map(Some)
        }

        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            // 普通 JSON 等反序列化器不识别该协议时，保持显式转换器的原有行为。
            super::optional_scalar(deserializer, true)
        }
    }
    deserializer.deserialize_newtype_struct(
        OPTIONAL_QUERY_SCALAR,
        OptionalScalar(std::marker::PhantomData),
    )
}

struct Input {
    value: Value,
    source: Source,
}
impl Input {
    fn scalar(self) -> Self {
        match self.value {
            Value::QueryValues(items) => Self {
                value: items.into_iter().next().unwrap_or(Value::Null),
                source: self.source,
            },
            _ => self,
        }
    }
    /// 仅类型化对象和字典解释克隆元数据，动态文档保留原始键。
    fn typed_map<'de, V: Visitor<'de>>(
        mut self,
        fields: &[&str],
        ordered: bool,
        visitor: V,
    ) -> Result<V::Value, Error> {
        if let Source::Clone(references) = &self.source
            && let Value::Object(entries) = &mut self.value
        {
            if let Some((key, value)) = entries.first()
                && key == "$id"
            {
                let Value::String(id) = value else {
                    return Err(de::Error::custom("克隆对象 $id 必须是字符串"));
                };
                references.register(id)?;
                entries.remove(0);
            }
            if entries.iter().any(|(key, _)| key.starts_with('$')) {
                return Err(de::Error::custom("克隆对象元数据无效或顺序错误"));
            }
        }
        self.map_with_order(fields, ordered, visitor)
    }
    fn map<'de, V: Visitor<'de>>(self, fields: &[&str], visitor: V) -> Result<V::Value, Error> {
        self.map_with_order(fields, false, visitor)
    }
    fn map_with_order<'de, V: Visitor<'de>>(
        self,
        fields: &[&str],
        ordered: bool,
        visitor: V,
    ) -> Result<V::Value, Error> {
        let Value::Object(entries) = self.value else {
            return Err(de::Error::custom("应提供对象"));
        };
        let mut result: Vec<(String, Value)> = Vec::new();
        let mut indexes: HashMap<String, usize> = HashMap::new();
        for (key, value) in entries {
            let key = fields
                .iter()
                .find(|name| match self.source {
                    Source::Clone(_) => **name == key,
                    _ => crate::string_helper::eq_ordinal_ignore_case(name, &key),
                })
                .map(|name| (*name).to_owned())
                .unwrap_or(key);
            if matches!(self.source, Source::WebProtocol)
                || (ordered && !matches!(self.source, Source::Query))
            {
                result.push((key, value));
                continue;
            }
            if let Some(&index) = indexes.get(&key) {
                match self.source {
                    Source::Json | Source::WebProtocol | Source::Object | Source::Clone(_) => {
                        result[index].1 = value
                    }
                    Source::Query => {
                        if let Value::QueryValues(values) = &mut result[index].1 {
                            values.push(value);
                        }
                    }
                }
            } else {
                indexes.insert(key.clone(), result.len());
                result.push((
                    key,
                    match self.source {
                        Source::Json | Source::WebProtocol | Source::Object | Source::Clone(_) => {
                            value
                        }
                        Source::Query => Value::QueryValues(vec![value]),
                    },
                ));
            }
        }
        visitor.visit_map(Fields {
            entries: result.into_iter(),
            pending: None,
            source: self.source,
        })
    }
}

struct Fields {
    entries: std::vec::IntoIter<(String, Value)>,
    pending: Option<(String, Value)>,
    source: Source,
}
impl<'de> MapAccess<'de> for Fields {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        match self.entries.next() {
            Some((key, value)) => {
                self.pending = Some((key.clone(), value));
                seed.deserialize(key.into_deserializer()).map(Some)
            }
            None => Ok(None),
        }
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let (key, value) = self
            .pending
            .take()
            .ok_or_else(|| <Error as de::Error>::custom("缺少字段值"))?;
        // 单值转换使用首项；错误尝试值按 StringValues.ToString 合并并跳过空项。
        let attempted = match &value {
            Value::QueryValues(values) if values.len() > 1 => {
                Some(crate::string_helper::join_string_values(
                    values.iter().filter_map(|value| match value {
                        Value::String(value) => Some(value.as_str()),
                        _ => None,
                    }),
                ))
            }
            _ => None,
        };
        seed.deserialize(Input {
            value,
            source: self.source.clone(),
        })
        .map_err(|mut error| {
            if let Some(value) = attempted
                && let InputErrorKind::InvalidQueryScalar {
                    attempted_value, ..
                } = &mut error.kind
            {
                *attempted_value = value;
            }
            error.with_field(key)
        })
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}
struct Items {
    entries: std::vec::IntoIter<Value>,
    source: Source,
}
impl<'de> SeqAccess<'de> for Items {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.entries
            .next()
            .map(|value| {
                seed.deserialize(Input {
                    value,
                    source: self.source.clone(),
                })
            })
            .transpose()
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

macro_rules! number {
    ($method:ident, $ty:ty, $visit:ident) => {
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            let this = self.scalar();
            match this.value {
                Value::String(value)
                    if !matches!(this.source, Source::Object | Source::Clone(_))
                        || stringify!($ty) == "i64" =>
                {
                    let named_float = matches!(this.source, Source::Json | Source::WebProtocol)
                        && matches!(stringify!($ty), "f32" | "f64")
                        && matches!(value.as_str(), "NaN" | "Infinity" | "-Infinity");
                    let number: $ty = if matches!(this.source, Source::Query) {
                        <$ty as QueryNumber>::parse_query(&value)
                    } else if matches!(this.source, Source::WebProtocol)
                        || (matches!(this.source, Source::Json) && stringify!($ty) != "i64")
                    {
                        value.parse().ok()
                    } else {
                        value.trim().parse().ok()
                    }
                    .ok_or_else(|| {
                        if matches!(this.source, Source::Query) {
                            InputError::invalid_query_scalar(value, "数字格式或范围无效")
                        } else {
                            <Error as de::Error>::custom("数字格式或范围无效")
                        }
                    })?;
                    if !(number as f64).is_finite()
                        && !named_float
                        && !(matches!(this.source, Source::Query)
                            && <$ty as QueryNumber>::ALLOW_NON_FINITE)
                    {
                        return Err(de::Error::custom("不支持非有限数字"));
                    }
                    visitor.$visit(number)
                }
                Value::Number(value) => serde_json::Value::Number(value)
                    .$method(visitor)
                    .map_err(de::Error::custom),
                _ => Err(de::Error::custom("应提供数字或数字字符串")),
            }
        }
    };
}

impl<'de> Deserializer<'de> for Input {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let this = self.scalar();
        match this.value {
            Value::Null => visitor.visit_unit(),
            Value::Bool(v) => visitor.visit_bool(v),
            Value::Number(v) => serde_json::Value::Number(v)
                .deserialize_any(visitor)
                .map_err(de::Error::custom),
            Value::String(v) => visitor.visit_string(v),
            Value::Array(v) => visitor.visit_seq(Items {
                entries: v.into_iter(),
                source: this.source,
            }),
            Value::Object(_) => this.map(&[], visitor),
            Value::QueryValues(_) => unreachable!("标量已取首个查询值"),
        }
    }
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        let fields = if name.starts_with("$protocol:case_sensitive:") {
            &[]
        } else {
            fields
        };
        self.scalar()
            .typed_map(fields, name.starts_with("$protocol:"), visitor)
    }
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.scalar().typed_map(&[], true, visitor)
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }
    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let this = self.scalar();
        match this.value {
            Value::Bool(v) => visitor.visit_bool(v),
            Value::String(v) if matches!(this.source, Source::Query) => {
                // Boolean.TryParse 去除两端 Unicode 空白和 NUL；此规则不用于其他标量。
                let value = v.trim_matches(|value: char| value.is_whitespace() || value == '\0');
                if value.eq_ignore_ascii_case("true") {
                    visitor.visit_bool(true)
                } else if value.eq_ignore_ascii_case("false") {
                    visitor.visit_bool(false)
                } else {
                    Err(InputError::invalid_query_scalar(
                        v,
                        "布尔参数应为 true 或 false",
                    ))
                }
            }
            _ => Err(de::Error::custom("应提供布尔值")),
        }
    }
    number!(deserialize_i8, i8, visit_i8);
    number!(deserialize_i16, i16, visit_i16);
    number!(deserialize_i32, i32, visit_i32);
    number!(deserialize_i64, i64, visit_i64);
    number!(deserialize_i128, i128, visit_i128);
    number!(deserialize_u8, u8, visit_u8);
    number!(deserialize_u16, u16, visit_u16);
    number!(deserialize_u32, u32, visit_u32);
    number!(deserialize_u64, u64, visit_u64);
    number!(deserialize_u128, u128, visit_u128);
    number!(deserialize_f32, f32, visit_f32);
    number!(deserialize_f64, f64, visit_f64);
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_string(visitor)
    }
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let this = self.scalar();
        match this.value {
            Value::String(v)
                if !matches!(
                    this.source,
                    Source::Object | Source::Clone(_) | Source::Json | Source::WebProtocol
                ) && v.trim().is_empty() =>
            {
                Err(InputError {
                    kind: InputErrorKind::RequiredString,
                    path: Vec::new(),
                    message: "必填字符串不可以为空".to_owned(),
                })
            }
            Value::String(v) => visitor.visit_string(v),
            _ => Err(de::Error::custom("应提供字符串")),
        }
    }
    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.scalar().value {
            Value::String(v) => {
                let mut chars = v.chars();
                match (chars.next(), chars.next()) {
                    (Some(v), None) => visitor.visit_char(v),
                    _ => Err(de::Error::custom("应提供单个字符")),
                }
            }
            _ => Err(de::Error::custom("应提供字符")),
        }
    }
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let value = match self.value {
            Value::Object(mut entries) if matches!(self.source, Source::Clone(_)) => {
                // ReferenceHandler.Preserve 对 List<T> 使用有序的 $id/$values 包装。
                // 只在目标是集合时解释元数据，不改写动态 JSON 或字典内容。
                if entries.len() != 2 {
                    return Err(de::Error::custom("克隆集合需要 $id 和 $values"));
                }
                let (values_key, values) = entries.pop().expect("已检查集合元数据长度");
                let (id_key, id) = entries.pop().expect("已检查集合元数据长度");
                if id_key != "$id" || !matches!(id, Value::String(_)) || values_key != "$values" {
                    return Err(de::Error::custom("克隆集合元数据无效或顺序错误"));
                }
                if let Source::Clone(references) = &self.source
                    && let Value::String(id) = &id
                {
                    references.register(id)?;
                }
                values
            }
            value => value,
        };
        match value {
            Value::Array(items) | Value::QueryValues(items) => {
                let mut items = Items {
                    entries: items.into_iter(),
                    source: self.source,
                };
                let result = visitor.visit_seq(&mut items)?;
                if items.entries.len() != 0 {
                    return Err(de::Error::custom("数组元素数量超出目标类型"));
                }
                Ok(result)
            }
            _ => Err(de::Error::custom("应提供数组")),
        }
    }
    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        // 固定数组由 Serde 的 tuple 入口读取，不具有 List<T> 的引用元数据语义
        if matches!(self.source, Source::Clone(_)) && matches!(self.value, Value::Object(_)) {
            return Err(de::Error::custom("克隆固定数组不接受对象引用包装"));
        }
        self.deserialize_seq(visitor)
    }
    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_tuple(len, visitor)
    }
    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.scalar().value {
            Value::Null => visitor.visit_unit(),
            _ => Err(de::Error::custom("应提供 null")),
        }
    }
    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        if name == OPTIONAL_QUERY_DATE_TIME && matches!(self.source, Source::Query) {
            let this = self.scalar();
            if matches!(&this.value, Value::Null)
                || matches!(&this.value, Value::String(value) if value.trim().is_empty())
            {
                return visitor.visit_none();
            }
            let Value::String(value) = this.value else {
                return Err(de::Error::custom("应提供日期字符串"));
            };
            let date = query_date_time::parse(&value)
                .ok_or_else(|| InputError::invalid_query_scalar(value, "日期格式或范围无效"))?;
            return visitor.visit_some(date.to_rfc3339().into_deserializer());
        }
        if name == QUERY_GUID && matches!(self.source, Source::Query) {
            let this = self.scalar();
            let Value::String(value) = this.value else {
                return Err(de::Error::custom("应提供 Guid 字符串"));
            };
            let guid = crate::data_helper::guid::parse(&value)
                .ok_or_else(|| InputError::invalid_query_scalar(value, "Guid 格式无效"))?;
            return visitor.visit_newtype_struct(guid.to_string().into_deserializer());
        }
        if name == OPTIONAL_QUERY_SCALAR && matches!(self.source, Source::Query) {
            let this = self.scalar();
            if matches!(&this.value, Value::Null)
                || matches!(&this.value, Value::String(value) if value.trim().is_empty())
            {
                return visitor.visit_none();
            }
            return visitor.visit_some(this);
        }
        const RAW_VALUE: &str = "$serde_json::private::RawValue";
        if name == RAW_VALUE {
            let json = serde_json::to_string(&self.scalar().value).map_err(de::Error::custom)?;
            return visitor.visit_map(de::value::MapDeserializer::new(
                [(RAW_VALUE, json)].into_iter(),
            ));
        }
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        let this = self.scalar();
        match this.value {
            Value::String(value) => de::value::StringDeserializer::<Error>::new(value)
                .deserialize_enum(name, variants, visitor),
            Value::Object(mut entries) if entries.len() == 1 => {
                let (variant, value) = entries.pop().expect("已检查枚举对象只有一项");
                visitor.visit_enum(EnumValue {
                    variant,
                    value,
                    source: this.source,
                })
            }
            _ => Err(de::Error::custom("应提供枚举字符串或单项枚举对象")),
        }
    }
    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_string(visitor)
    }
    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }
    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }
    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }
}

struct EnumValue {
    variant: String,
    value: Value,
    source: Source,
}
struct VariantValue {
    value: Value,
    source: Source,
}
impl<'de> de::EnumAccess<'de> for EnumValue {
    type Error = Error;
    type Variant = VariantValue;
    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        let variant = seed.deserialize(self.variant.into_deserializer())?;
        Ok((
            variant,
            VariantValue {
                value: self.value,
                source: self.source,
            },
        ))
    }
}
impl<'de> de::VariantAccess<'de> for VariantValue {
    type Error = Error;
    fn unit_variant(self) -> Result<(), Error> {
        <()>::deserialize(Input {
            value: self.value,
            source: self.source,
        })
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(Input {
            value: self.value,
            source: self.source,
        })
    }
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        Input {
            value: self.value,
            source: self.source,
        }
        .deserialize_tuple(len, visitor)
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        Input {
            value: self.value,
            source: self.source,
        }
        .map(fields, visitor)
    }
}
