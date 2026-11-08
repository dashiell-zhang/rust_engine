pub mod date_time;
mod defined_bmp;
mod encoding;
pub mod input;
mod output;

pub(crate) const MAX_DEPTH: usize = 64;

use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned, de::Error as DeError};
use serde_json::{Map, Value};

#[derive(Deserialize)]
#[serde(untagged)]
enum I64OrString {
    I64(i64),
    String(String),
}

/// WebAPI 可空标量：null、空字符串及纯空白字符串均视为无值。
/// 仅用于请求 DTO 的可空标量字段，不用于数组、对象或第三方协议模型。
pub fn deserialize_optional_json_scalar<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    optional_scalar(deserializer, false)
}

/// Query 的可空标量还允许大小写不敏感的布尔文本。
pub fn deserialize_optional_query_scalar<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    input::optional_query_scalar(deserializer)
}

fn optional_scalar<'de, D, T>(deserializer: D, query: bool) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = input::deserialize_json_value(deserializer)?;
    if value.is_null() || value.as_str().is_some_and(|text| text.trim().is_empty()) {
        return Ok(None);
    }
    if query {
        return input::query_scalar(value)
            .map(Some)
            .map_err(D::Error::custom);
    }
    input::json_scalar(value)
        .map(Some)
        .map_err(D::Error::custom)
}

/// JSON 对象字符串错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonObjectStringError {
    /// JSON 语法不合法
    InvalidJson,

    /// JSON 根节点不是对象
    NotObject,
}

/// 通过 Key 获取 JSON 中的字符串值
pub fn get_value_by_key(json: &str, key: &str) -> Option<String> {
    let values: std::collections::HashMap<String, &serde_json::value::RawValue> =
        serde_json::from_str(json).ok()?;
    let raw = values.get(key)?.get();
    match raw {
        "null" => Some(String::new()),
        "true" => Some("True".into()),
        "false" => Some("False".into()),
        _ if raw.starts_with('"') => serde_json::from_str(raw).ok(),
        _ => Some(raw.to_owned()),
    }
}

/// 对象转 JSON
pub fn object_to_json<T>(value: &T) -> Result<String, serde_json::Error>
where
    T: Serialize,
{
    output::to_json::<_, true>(value)
}

/// 默认 Web JSON（事件）：保留数字 long，使用默认编码器，不套用内部日期转换。
pub fn web_object_to_json<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    output::to_web_json(value)
}

/// MVC 控制器输出：i64 值写为字符串、日期保持 ISO，拒绝非有限浮点数。
/// JSON 文档值保留自身类型，字典键沿用目标序列化器；流式结果可逐项写入。
pub fn write_controller_json<W: std::io::Write, T: Serialize + ?Sized>(
    writer: W,
    value: &T,
) -> Result<(), serde_json::Error> {
    output::write_controller_json::<_, _, false>(writer, value)
}

/// 流式控制器数组的单项：外层数组已由宿主写出，仍占用一层 JSON 深度。
pub fn write_controller_json_array_item<W: std::io::Write, T: Serialize + ?Sized>(
    writer: W,
    value: &T,
) -> Result<(), serde_json::Error> {
    output::write_controller_json::<_, _, true>(writer, value)
}

/// 对象克隆序列化为 Json
pub fn object_clone_json<T>(value: &T) -> Result<String, serde_json::Error>
where
    T: Serialize,
{
    output::to_json::<_, false>(value)
}

/// Json 转对象
pub fn json_to_object<T>(json: &str) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
{
    input::object(json)
}

/// Json 克隆反序列化为对象
pub fn json_clone_object<T>(json: &str) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
{
    input::clone_object(json)
}

/// 兼容前端将 long 作为字符串传递的 JSON 反序列化。
pub fn deserialize_i64_from_string_or_number<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    match I64OrString::deserialize(deserializer)? {
        I64OrString::I64(value) => Ok(value),
        I64OrString::String(value) => value.trim().parse::<i64>().map_err(D::Error::custom),
    }
}

/// 兼容前端将可选 long 作为字符串传递的 JSON 反序列化。
pub fn deserialize_option_i64_from_string_or_number<'de, D>(
    deserializer: D,
) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<I64OrString>::deserialize(deserializer)?;

    match value {
        Some(I64OrString::I64(value)) => Ok(Some(value)),
        Some(I64OrString::String(value)) => {
            let value = value.trim();
            if value.is_empty() {
                Ok(None)
            } else {
                value.parse::<i64>().map(Some).map_err(D::Error::custom)
            }
        }
        None => Ok(None),
    }
}

/// 校验可选 JSON 字符串是否为对象
pub fn validate_optional_json_object_string(
    json: Option<&str>,
) -> Result<(), JsonObjectStringError> {
    parse_optional_json_object_string(json).map(|_| ())
}

/// 解析可选 JSON 字符串为对象映射
pub fn parse_optional_json_object_string(
    json: Option<&str>,
) -> Result<Option<Map<String, Value>>, JsonObjectStringError> {
    let Some(json) = json.map(str::trim).filter(|item| !item.is_empty()) else {
        return Ok(None);
    };

    let value = input::parse_json_value(json).map_err(|_| JsonObjectStringError::InvalidJson)?;

    parse_optional_json_object_value(Some(value))
}

/// 解析可选 JSON 值为对象映射
pub fn parse_optional_json_object_value(
    value: Option<Value>,
) -> Result<Option<Map<String, Value>>, JsonObjectStringError> {
    match value {
        None => Ok(None),
        Some(Value::Object(map)) => Ok(Some(map)),
        Some(_) => Err(JsonObjectStringError::NotObject),
    }
}

/// 克隆对象
pub fn clone<T>(value: &T) -> Result<T, serde_json::Error>
where
    T: Serialize + DeserializeOwned,
{
    json_clone_object(&object_clone_json(value)?)
}
