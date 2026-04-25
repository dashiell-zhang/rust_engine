use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned, de::Error as DeError};
use serde_json::{Map, Value};

#[derive(Deserialize)]
#[serde(untagged)]
enum I64OrString {
    I64(i64),
    String(String),
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
    let value: Value = serde_json::from_str(json).ok()?;
    value.get(key).map(json_value_to_string)
}

/// 对象转 JSON
pub fn object_to_json<T>(value: &T) -> Result<String, serde_json::Error>
where
    T: Serialize,
{
    serde_json::to_string(value)
}

/// 对象克隆序列化为 Json
pub fn object_clone_json<T>(value: &T) -> Result<String, serde_json::Error>
where
    T: Serialize,
{
    serde_json::to_string(value)
}

/// Json 转对象
pub fn json_to_object<T>(json: &str) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
{
    serde_json::from_str(json)
}

/// Json 克隆反序列化为对象
pub fn json_clone_object<T>(json: &str) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
{
    serde_json::from_str(json)
}

/// 兼容前端将 long 作为字符串传递的 JSON 反序列化。
pub fn deserialize_i64_from_string_or_number<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    match I64OrString::deserialize(deserializer)? {
        I64OrString::I64(value) => Ok(value),
        I64OrString::String(value) => value.parse::<i64>().map_err(D::Error::custom),
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
            value.parse::<i64>().map(Some).map_err(D::Error::custom)
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

    let value: Value =
        serde_json::from_str(json).map_err(|_| JsonObjectStringError::InvalidJson)?;

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
    let payload = serde_json::to_vec(value)?;
    serde_json::from_slice(&payload)
}

/// 将 JSON 值尽量还原为直观文本
fn json_value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}
