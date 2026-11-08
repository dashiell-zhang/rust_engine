use serde::{Deserialize, Deserializer};

/// JSON 中的长整数允许以字符串或数字传入，返回模型仍保持字符串 ID。
pub(crate) fn id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    common::json_helper::deserialize_i64_from_string_or_number(deserializer)
        .map(|value| value.to_string())
}

pub(crate) fn zero_id() -> String {
    "0".into()
}

/// 输出可为空的字段在作为请求输入时仍可声明为必填。
pub(crate) fn required_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

pub(crate) fn default_date_time() -> String {
    "0001-01-01T00:00:00+00:00".into()
}

pub(crate) fn date_time<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    common::json_helper::date_time::deserialize_iso_offset_string(deserializer)
}
