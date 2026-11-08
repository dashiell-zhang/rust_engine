use super::{Input, InputError, InputErrorKind, Source, Value};
use serde::de::DeserializeOwned;

/// WebAPI 正文模型的验证声明；内部 JSON 与第三方协议不自动加入此验证。
pub trait JsonModel: DeserializeOwned {
    /// 对外 JSON 协议中的模型类型名称，不暴露 Rust 模块实现路径。
    fn json_type_name() -> &'static str;

    fn json_fields() -> &'static [JsonField] {
        &[]
    }
}

#[derive(Clone, Copy)]
pub struct JsonObject {
    pub name: &'static str,
    pub fields: &'static [JsonField],
}

/// HTTP 字段的 token 约束和验证分类；实际值转换仍复用字段的 Serde 声明。
pub enum JsonFieldKind {
    String,
    Boolean,
    Collection,
    Dictionary,
    Object,
    Value,
}

/// 原始标量交给字段既有转换器校验，不公开内部 JSON 表示
pub struct JsonScalar(pub(super) Value);

impl JsonScalar {
    pub fn into_deserializer<'de>(self) -> impl serde::Deserializer<'de, Error = InputError> {
        Input {
            value: self.0,
            source: Source::Json,
        }
    }
}

/// 在 token 读取阶段校验数值，保留首次转换失败的位置和协议类型
#[derive(Clone, Copy)]
pub struct JsonScalarField {
    pub type_name: &'static str,
    pub read: fn(JsonScalar) -> Result<(), InputError>,
    /// 源可空转换器通过独立 JsonSerializer 读取当前值，错误路径及位置由此值起算
    pub scoped: bool,
}

/// 属性名称、必填提示及集合约束由模型声明，HTTP 层负责组织错误响应。
pub struct JsonField {
    pub names: &'static [&'static str],
    pub property: &'static str,
    pub kind: JsonFieldKind,
    pub required: Option<&'static str>,
    pub required_on_missing: bool,
    pub min_length: Option<(usize, &'static str)>,
    pub nested: Option<fn() -> JsonObject>,
    pub type_name: Option<fn() -> String>,
    pub element_kind: JsonFieldKind,
    pub scalar: Option<JsonScalarField>,
    pub element_scalar: Option<JsonScalarField>,
}

pub(super) fn prepare(
    value: &mut Value,
    fields: &[JsonField],
    prefix: &str,
    errors: &mut Vec<InputError>,
) {
    let Value::Object(entries) = value else {
        return;
    };
    for field in fields {
        let path = if prefix.is_empty() {
            field.property.to_owned()
        } else {
            format!("{prefix}.{}", field.property)
        };
        let index = entries.iter().rposition(|(name, _)| {
            field
                .names
                .iter()
                .any(|candidate| crate::string_helper::eq_ordinal_ignore_case(name, candidate))
        });
        let missing = index.is_none();
        let empty = match index.map(|index| &entries[index].1) {
            None => field.required_on_missing,
            Some(Value::Null) => true,
            Some(Value::String(value)) if matches!(field.kind, JsonFieldKind::String) => {
                value.trim().is_empty()
            }
            _ => false,
        };
        if empty && let Some(message) = field.required {
            let message = if message.is_empty() {
                format!("The {} field is required.", field.property)
            } else {
                message.to_owned()
            };
            errors.push(validation_error(path.clone(), message));
            // 无效引用值仅临时占位以完成其他字段的格式转换；有验证错误时不交付模型。
            let mut placeholder = match field.kind {
                JsonFieldKind::String => Value::String(String::new()),
                JsonFieldKind::Collection => Value::Array(Vec::new()),
                JsonFieldKind::Object | JsonFieldKind::Dictionary => Value::Object(Vec::new()),
                JsonFieldKind::Value | JsonFieldKind::Boolean => Value::Null,
            };
            if matches!(field.kind, JsonFieldKind::Object)
                && let Some(nested) = field.nested
            {
                // 缺少整个对象时仅报告父属性；子字段占位不加入调用方的验证结果。
                prepare(&mut placeholder, nested().fields, &path, &mut Vec::new());
            }
            if let Some(index) = index {
                entries[index].1 = placeholder;
            } else {
                entries.push((field.names[0].to_owned(), placeholder));
            }
            continue;
        }
        if let Some((minimum, message)) = field.min_length {
            let length = match index.map(|index| &entries[index].1) {
                Some(Value::Array(values)) => Some(values.len()),
                Some(Value::String(value)) => Some(value.encode_utf16().count()),
                None if !field.required_on_missing => Some(0),
                _ => None,
            };
            if length.is_some_and(|length| length < minimum) {
                errors.push(validation_error(path.clone(), message.to_owned()));
            }
        }
        if !missing && let (Some(index), Some(nested)) = (index, field.nested) {
            if matches!(field.kind, JsonFieldKind::Dictionary) {
                if let Value::Object(values) = &mut entries[index].1 {
                    for (key, value) in values {
                        prepare(value, nested().fields, &format!("{path}[{key}]"), errors);
                    }
                }
            } else {
                prepare_nested(&mut entries[index].1, nested().fields, &path, errors);
            }
        }
    }
}

fn prepare_nested(
    value: &mut Value,
    fields: &[JsonField],
    path: &str,
    errors: &mut Vec<InputError>,
) {
    if let Value::Array(values) = value {
        for (index, value) in values.iter_mut().enumerate() {
            prepare(value, fields, &format!("{path}[{index}]"), errors);
        }
    } else {
        prepare(value, fields, path, errors);
    }
}

fn validation_error(path: String, message: String) -> InputError {
    InputError {
        kind: InputErrorKind::Validation,
        path: vec![path],
        message,
    }
}
