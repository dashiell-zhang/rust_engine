use serde_json::{Value, json};
use utoipa::{
    Modify,
    openapi::{
        OpenApi,
        security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    },
};

/// Bearer JWT 安全方案注入器
pub struct BearerAuth;

/// 对 OpenAPI 文档应用统一增强
pub fn apply_openapi_enhancements(openapi: &mut OpenApi) {
    BearerAuth.modify(openapi);

    let Ok(mut value) = serde_json::to_value(&*openapi) else {
        return;
    };

    enhance_openapi_json_value(&mut value);

    if let Ok(updated) = serde_json::from_value(value) {
        *openapi = updated;
    }
}

/// Bearer JWT 安全方案文档适配
impl Modify for BearerAuth {
    /// 向 OpenAPI 文档注入 Bearer 安全定义
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

#[derive(Clone)]
enum JsonPathSegment {
    Key(String),
    Index(usize),
}

/// 递归增强 OpenAPI JSON 结构中的枚举 schema
fn enhance_openapi_json_value(value: &mut Value) {
    let mut stack: Vec<Vec<JsonPathSegment>> = vec![Vec::new()];

    while let Some(path) = stack.pop() {
        let Some(current) = get_json_value_mut(value, &path) else {
            continue;
        };

        match current {
            Value::Array(items) => {
                let child_count = items.len();
                for index in 0..child_count {
                    let mut child_path = path.clone();
                    child_path.push(JsonPathSegment::Index(index));
                    stack.push(child_path);
                }
            }
            Value::Object(map) => {
                let child_keys = map.keys().cloned().collect::<Vec<_>>();
                for key in child_keys {
                    let mut child_path = path.clone();
                    child_path.push(JsonPathSegment::Key(key));
                    stack.push(child_path);
                }

                let Some(Value::Array(enum_values)) = map.get("enum") else {
                    continue;
                };

                if enum_values.is_empty() {
                    continue;
                }

                let enum_values = enum_values.clone();

                let display_values = enum_values
                    .iter()
                    .map(format_enum_value)
                    .collect::<Vec<_>>();
                let enum_summary = format!("可选值: {}", display_values.join(" | "));

                let description = map
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim();

                let merged_description = if description.is_empty() {
                    enum_summary.clone()
                } else if description.contains(&enum_summary) {
                    description.to_string()
                } else {
                    format!("{description}\n\n{enum_summary}")
                };

                map.insert("description".to_string(), Value::String(merged_description));

                map.entry("x-enumValues".to_string())
                    .or_insert_with(|| Value::Array(enum_values.clone()));
                map.entry("x-enumNames".to_string()).or_insert_with(|| {
                    Value::Array(display_values.iter().cloned().map(Value::String).collect())
                });
                map.entry("x-enumDescriptions".to_string())
                    .or_insert_with(|| {
                        Value::Array(
                            display_values
                                .iter()
                                .map(|value| Value::String(format!("允许值 {value}")))
                                .collect(),
                        )
                    });
                map.entry("x-enum-enhanced".to_string())
                    .or_insert_with(|| json!(true));
            }
            _ => {}
        }
    }
}

fn get_json_value_mut<'a>(root: &'a mut Value, path: &[JsonPathSegment]) -> Option<&'a mut Value> {
    let mut current = root;

    for segment in path {
        match segment {
            JsonPathSegment::Key(key) => {
                current = current.as_object_mut()?.get_mut(key)?;
            }
            JsonPathSegment::Index(index) => {
                current = current.as_array_mut()?.get_mut(*index)?;
            }
        }
    }

    Some(current)
}

/// 格式化枚举值用于文档展示
fn format_enum_value(value: &Value) -> String {
    match value {
        Value::String(text) => format!("\"{text}\""),
        _ => value.to_string(),
    }
}
