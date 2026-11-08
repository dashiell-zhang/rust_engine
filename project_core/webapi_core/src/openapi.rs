use serde_json::{Value, json};
use utoipa::{
    Modify,
    openapi::{
        Content, Header, OpenApi, RefOr, Response, Schema,
        path::{Operation, Parameter, PathItem},
        request_body::RequestBody,
        schema::{AdditionalProperties, ArrayItems},
        security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    },
};

/// Bearer JWT 安全方案注入器
pub struct BearerAuth;

/// 对 OpenAPI 文档应用统一增强；直接修改模型，避免 JSON 回读丢失整个增强结果。
pub fn apply_openapi_enhancements(openapi: &mut OpenApi) {
    BearerAuth.modify(openapi);
    if let Some(components) = &mut openapi.components {
        for schema in components.schemas.values_mut().filter_map(inline_mut) {
            enhance_schema(schema);
        }
        for response in components.responses.values_mut().filter_map(inline_mut) {
            enhance_response(response);
        }
        for parameter in components.parameters.values_mut().filter_map(inline_mut) {
            enhance_parameter(parameter);
        }
        for body in components
            .request_bodies
            .values_mut()
            .filter_map(inline_mut)
        {
            enhance_request_body(body);
        }
        for header in components.headers.values_mut().filter_map(inline_mut) {
            enhance_header(header);
        }
        for content in components.media_types.values_mut().filter_map(inline_mut) {
            enhance_content(content);
        }
        for path in components.path_items.values_mut() {
            enhance_path(path);
        }
    }
    for path in openapi.paths.paths.values_mut() {
        enhance_path(path);
    }
}

impl Modify for BearerAuth {
    fn modify(&self, openapi: &mut OpenApi) {
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

fn inline_mut<T>(value: &mut RefOr<T>) -> Option<&mut T> {
    match value {
        RefOr::T(value) => Some(value),
        RefOr::Ref(_) => None,
    }
}

fn enhance_path(path: &mut PathItem) {
    for parameter in path.parameters.iter_mut().flatten().filter_map(inline_mut) {
        enhance_parameter(parameter);
    }
    for operation in [
        &mut path.get,
        &mut path.post,
        &mut path.put,
        &mut path.patch,
        &mut path.delete,
        &mut path.head,
        &mut path.options,
        &mut path.trace,
        &mut path.query,
    ]
    .into_iter()
    .filter_map(Option::as_mut)
    .chain(path.additional_operations.values_mut())
    {
        enhance_error_responses(operation);
        enhance_authorization_responses(operation);
        for parameter in operation
            .parameters
            .iter_mut()
            .flatten()
            .filter_map(inline_mut)
        {
            enhance_parameter(parameter);
        }
        if let Some(body) = operation.request_body.as_mut().and_then(inline_mut) {
            enhance_request_body(body);
        }
        for response in operation
            .responses
            .responses
            .values_mut()
            .filter_map(inline_mut)
        {
            enhance_response(response);
        }
    }
}

fn enhance_error_responses(operation: &mut Operation) {
    let responses = &mut operation.responses.responses;
    if let Some(response) = responses.get_mut("500").and_then(inline_mut) {
        response.description = "Development 环境的内部异常；其他环境返回 400 和通用 errMsg".into();
        let response = responses
            .entry("400".into())
            .or_insert_with(|| Response::new("").into());
        if let Some(response) = inline_mut(response) {
            let description = "非 Development 环境的内部异常返回系统全局内部异常";
            if !response.description.contains(description) {
                if !response.description.is_empty() {
                    response.description.push('；');
                }
                response.description.push_str(description);
            }
        }
    }
}

/// 统一记录共享权限守卫的拒绝响应及续签头，避免各业务接口遗漏。
fn enhance_authorization_responses(operation: &mut Operation) {
    let requires_bearer = operation.security.as_ref().is_some_and(|requirements| {
        !requirements.is_empty()
            && requirements.iter().all(|requirement| {
                serde_json::to_value(requirement)
                    .is_ok_and(|value| value.get("bearer_auth").is_some())
            })
    });
    if !requires_bearer {
        return;
    }
    let responses = &mut operation.responses.responses;
    responses
        .entry("403".into())
        .or_insert_with(|| Response::new("已认证，但没有访问此路由的权限；响应体为空").into());
    if let Some(response) = responses.get_mut("401").and_then(inline_mut) {
        response.description = "身份认证或请求签名校验失败".into();
        response
            .headers
            .entry("WWW-Authenticate".into())
            .or_insert_with(|| {
                let mut header = Header::default();
                header.description =
                    Some("JWT 认证失败时返回 Bearer 挑战；签名过滤器的 401 不包含该头".into());
                header.into()
            });
    }
    for (status, response) in responses.iter_mut() {
        let Some(response) = inline_mut(response) else {
            continue;
        };
        if status == "500" {
            continue;
        }
        response
            .headers
            .entry("NewToken".into())
            .or_insert_with(|| {
                let mut header = Header::default();
                header.description = Some(
                    "本次请求完成续签时返回的新 JWT；权限拒绝或业务校验失败时也可能返回".into(),
                );
                header.into()
            });
    }
}

fn enhance_parameter(parameter: &mut Parameter) {
    if let Some(schema) = parameter.schema.as_mut().and_then(inline_mut) {
        enhance_schema(schema);
    }
    for content in parameter.content.values_mut().filter_map(inline_mut) {
        enhance_content(content);
    }
}

fn enhance_request_body(body: &mut RequestBody) {
    for content in body.content.values_mut().filter_map(inline_mut) {
        enhance_content(content);
    }
}

fn enhance_header(header: &mut Header) {
    if let Some(schema) = header.schema.as_mut().and_then(inline_mut) {
        enhance_schema(schema);
    }
    for content in header.content.values_mut().filter_map(inline_mut) {
        enhance_content(content);
    }
}

fn enhance_response(response: &mut Response) {
    for header in response.headers.values_mut().filter_map(inline_mut) {
        enhance_header(header);
    }
    for content in response.content.values_mut().filter_map(inline_mut) {
        enhance_content(content);
    }
}

fn enhance_content(content: &mut Content) {
    for schema in [&mut content.schema, &mut content.item_schema]
        .into_iter()
        .filter_map(Option::as_mut)
        .filter_map(inline_mut)
    {
        enhance_schema(schema);
    }
}

/// 遍历内联 schema；引用保留原样，由组件定义统一增强。
fn enhance_schema(schema: &mut Schema) {
    let mut stack = vec![schema];
    while let Some(schema) = stack.pop() {
        match schema {
            Schema::Object(object) => {
                if let Some(values) = object
                    .enum_values
                    .as_ref()
                    .filter(|values| !values.is_empty())
                {
                    let names: Vec<_> = values.iter().map(format_enum_value).collect();
                    let summary = format!("可选值: {}", names.join(" | "));
                    let description = object.description.as_deref().unwrap_or_default().trim();
                    object.description = Some(if description.is_empty() {
                        summary.clone()
                    } else if description.contains(&summary) {
                        description.to_string()
                    } else {
                        format!("{description}\n\n{summary}")
                    });
                    let extensions = object.extensions.get_or_insert_with(Default::default);
                    extensions
                        .entry("x-enumValues".into())
                        .or_insert_with(|| json!(values));
                    extensions
                        .entry("x-enumNames".into())
                        .or_insert_with(|| json!(names));
                    extensions
                        .entry("x-enumDescriptions".into())
                        .or_insert_with(|| {
                            json!(
                                names
                                    .iter()
                                    .map(|value| format!("允许值 {value}"))
                                    .collect::<Vec<_>>()
                            )
                        });
                    extensions
                        .entry("x-enum-enhanced".into())
                        .or_insert(json!(true));
                }
                stack.extend(object.properties.values_mut().filter_map(inline_mut));
                if let Some(AdditionalProperties::RefOr(schema)) =
                    object.additional_properties.as_deref_mut()
                    && let Some(schema) = inline_mut(schema)
                {
                    stack.push(schema);
                }
                if let Some(schema) = object.property_names.as_deref_mut() {
                    stack.push(schema);
                }
                if let Some(schema) = object.content_schema.as_deref_mut().and_then(inline_mut) {
                    stack.push(schema);
                }
            }
            Schema::Array(array) => {
                if let ArrayItems::RefOrSchema(schema) = &mut array.items
                    && let Some(schema) = inline_mut(schema)
                {
                    stack.push(schema);
                }
                stack.extend(array.prefix_items.iter_mut());
            }
            Schema::OneOf(schema) => stack.extend(schema.items.iter_mut().filter_map(inline_mut)),
            Schema::AllOf(schema) => stack.extend(schema.items.iter_mut().filter_map(inline_mut)),
            Schema::AnyOf(schema) => stack.extend(schema.items.iter_mut().filter_map(inline_mut)),
            _ => {}
        }
    }
}

fn format_enum_value(value: &Value) -> String {
    match value {
        Value::String(text) => format!("\"{text}\""),
        _ => value.to_string(),
    }
}
