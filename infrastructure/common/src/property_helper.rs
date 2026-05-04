use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::HashMap;

/// 获取对象的属性字典
pub fn get_properties<T>(value: &T) -> Result<HashMap<String, Value>, serde_json::Error>
where
    T: Serialize,
{
    object_to_map(value)
}

/// 获取对象的属性显示名字典
pub fn get_properties_display_name<T>(
    value: &T,
) -> Result<HashMap<String, Value>, serde_json::Error>
where
    T: Serialize,
{
    object_to_map(value)
}

/// 比较两个实体并输出差异文本
pub fn comparison_entity<T>(original: &T, after: &T) -> Result<String, serde_json::Error>
where
    T: Serialize,
{
    let original = object_to_map(original)?;
    let after = object_to_map(after)?;
    let mut lines = Vec::new();
    let mut keys = original.keys().cloned().collect::<Vec<_>>();
    keys.sort();

    for key in keys {
        let Some(original_value) = original.get(&key) else {
            continue;
        };
        let Some(after_value) = after.get(&key) else {
            continue;
        };

        if original_value == after_value {
            continue;
        }

        let left = display_value(original_value);
        let right = display_value(after_value);
        lines.push(format!("{key}:{left} -> {right}； \n"));
    }

    Ok(lines.concat())
}

/// 同类型赋值
pub fn assignment<T>(left: &mut T, right: &T) -> Result<(), serde_json::Error>
where
    T: Serialize + DeserializeOwned,
{
    *left = clone(right)?;
    Ok(())
}

/// 克隆对象
pub fn clone<T>(value: &T) -> Result<T, serde_json::Error>
where
    T: Serialize + DeserializeOwned,
{
    let payload = serde_json::to_vec(value)?;
    serde_json::from_slice(&payload)
}

/// 不同类型赋值
pub fn assignment_different_type<L, R>(left: &mut L, right: &R) -> Result<(), serde_json::Error>
where
    L: DeserializeOwned,
    R: Serialize,
{
    let payload = serde_json::to_vec(right)?;
    *left = serde_json::from_slice(&payload)?;
    Ok(())
}

/// 对象转字典
fn object_to_map<T>(value: &T) -> Result<HashMap<String, Value>, serde_json::Error>
where
    T: Serialize,
{
    match serde_json::to_value(value)? {
        Value::Object(map) => Ok(map.into_iter().collect()),
        _ => Ok(HashMap::new()),
    }
}

/// 生成差异展示值
fn display_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => {
            if *value {
                "是".to_string()
            } else {
                "否".to_string()
            }
        }
        Value::String(value) => {
            if let Ok(date) = DateTime::parse_from_rfc3339(value) {
                return date.format("%Y-%m-%d").to_string();
            }
            if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
                return date.format("%Y-%m-%d").to_string();
            }
            if let Ok(date_time) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
                return date_time.format("%Y-%m-%d").to_string();
            }
            value.clone()
        }
        Value::Number(value) => value.to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}
