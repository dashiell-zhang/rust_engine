//! HTTP 层负责 URL 解码；字段遍历和模型绑定复用公共实现。

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use common::json_helper::input::{InputError, InputErrorKind, QueryModel};

/// 保留模型错误的字段身份，供控制器在所有参数绑定后统一排序和去重。
/// 只标记模型绑定失败；认证、媒体类型和正文读取错误不参与合并。
#[derive(Clone, Default)]
pub(crate) struct ModelErrors(pub Vec<(String, String)>);

impl ModelErrors {
    pub(crate) fn append_response(&mut self, response: &mut Response) -> bool {
        if let Some(errors) = response.extensions_mut().remove::<Self>() {
            self.0.extend(errors.0);
            true
        } else {
            false
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl IntoResponse for ModelErrors {
    fn into_response(self) -> Response {
        let mut response =
            crate::error::json_error_response(StatusCode::BAD_REQUEST, join_errors(self.0.clone()));
        response.extensions_mut().insert(self);
        response
    }
}

pub(crate) fn query<T: QueryModel>(query: &str) -> Result<T, Vec<InputError>> {
    T::bind_query(
        &url::form_urlencoded::parse(query.as_bytes())
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>(),
    )
}

/// 模型验证响应归属宿主；公共 JSON 组件只提供错误分类和字段路径。
pub(crate) fn model_errors(errors: &[InputError]) -> ModelErrors {
    ModelErrors(
        errors
            .iter()
            .map(|error| (error_key(error, None), scalar_error_message(error, None)))
            .collect(),
    )
}

fn error_key(error: &InputError, property: Option<&str>) -> String {
    let mut path = error.path().to_vec();
    if let InputErrorKind::MissingField(field) = error.kind() {
        path.push((*field).to_owned());
    }
    if let Some(property) = property
        && let Some(last) = path.last_mut()
    {
        *last = property.to_owned();
    }
    path.join(".")
}

/// ModelState 按树的广度枚举，同层先比较 UTF-16 长度，再按 OrdinalIgnoreCase。
/// 元组首项只用于排序与去重；第二项为包含字段名的完整响应文案。
pub(crate) fn join_errors(mut errors: Vec<(String, String)>) -> String {
    errors.sort_by_cached_key(|(key, _)| {
        let segments: Vec<_> = key
            .split(['.', '['])
            .filter(|part| !part.is_empty())
            .map(|part| {
                (
                    part.encode_utf16().count(),
                    common::string_helper::ordinal_ignore_case_key(part)
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        (segments.len(), segments)
    });
    let mut seen = std::collections::HashSet::new();
    errors
        .into_iter()
        .filter(|(key, _)| seen.insert(common::string_helper::ordinal_ignore_case_key(key)))
        .map(|(_, message)| message)
        .collect::<Vec<_>>()
        .join(" | ")
}

/// 复杂模型只在没有参数名前缀时回退无前缀属性，不能逐字段混用两种来源。
pub(crate) fn page_query<T>(query: &str) -> Result<(T, String), ModelErrors>
where
    T: application_model::shared::models::PageRequest,
{
    use common::string_helper::eq_ordinal_ignore_case;
    let fields: Vec<_> = url::form_urlencoded::parse(query.as_bytes()).collect();
    let prefixed = fields.iter().any(|(key, _)| {
        eq_ordinal_ignore_case(
            key.split(['.', '[']).next().unwrap_or_default(),
            T::QUERY_PARAMETER_NAME,
        )
    });
    let prefix = if prefixed {
        format!("{}.", T::QUERY_PARAMETER_NAME)
    } else {
        String::new()
    };
    let is_parameter = |key: &str| {
        T::QUERY_PARAMETERS
            .iter()
            .any(|name| eq_ordinal_ignore_case(key, name))
    };
    let fields = fields.into_iter().filter_map(|(key, value)| {
        if is_parameter(&key) {
            return Some((key.into_owned(), value.into_owned()));
        }
        if !prefixed {
            return Some((key.into_owned(), value.into_owned()));
        }
        let (parameter, property) = key.split_once('.')?;
        if eq_ordinal_ignore_case(parameter, T::QUERY_PARAMETER_NAME) && !is_parameter(property) {
            Some((property.to_owned(), value.into_owned()))
        } else {
            None
        }
    });
    T::bind_query(&fields.collect::<Vec<_>>())
        .map(|value| (value, prefix.clone()))
        .map_err(|errors| {
            ModelErrors(
                errors
                    .iter()
                    .map(|error| page_query_error_message::<T>(error, &prefix))
                    .collect(),
            )
        })
}

fn page_query_error_message<T: application_model::shared::models::PageRequest>(
    error: &InputError,
    prefix: &str,
) -> (String, String) {
    let property = error.path().last().and_then(|field| match field.as_str() {
        "page" => Some("Page"),
        "pageSize" => Some("PageSize"),
        _ => T::QUERY_PROPERTIES
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, property)| *property),
    });
    if property.is_some() {
        (
            format!("{prefix}{}", error_key(error, property)),
            format!("{prefix}{}", scalar_error_message(error, property)),
        )
    } else {
        (error_key(error, None), scalar_error_message(error, None))
    }
}

fn scalar_error_message(error: &InputError, property: Option<&str>) -> String {
    if matches!(error.kind(), InputErrorKind::Validation) {
        return format!("{} : {}", error_key(error, property), error.message());
    }
    if let InputErrorKind::Range { minimum, maximum } = error.kind() {
        let name = property
            .or_else(|| error.path().last().map(String::as_str))
            .unwrap_or_default();
        return format!(
            "{} : The field {name} must be between {minimum} and {maximum}.",
            error_key(error, property)
        );
    }
    if let InputErrorKind::MissingField(field) = error.kind() {
        // 没有 Option/default 的 Query 字段声明为必填；不推断其类型或补造值。
        let name = property.unwrap_or(field);
        let mut path = error.path().to_vec();
        path.push(name.to_owned());
        return format!("{} : The {name} field is required.", path.join("."));
    }
    if let InputErrorKind::InvalidQueryScalar {
        attempted_value,
        is_empty,
    } = error.kind()
    {
        let key = property
            .map(str::to_owned)
            .unwrap_or_else(|| error.path().join("."));
        let message = if *is_empty {
            format!("The value '{attempted_value}' is invalid.")
        } else if let Some(property) = property {
            format!("The value '{attempted_value}' is not valid for {property}.")
        } else {
            format!("The value '{attempted_value}' is not valid.")
        };
        return format!("{key} : {message}");
    }
    if matches!(error.kind(), InputErrorKind::RequiredString)
        && let Some(field) = error.path().last()
    {
        return format!(
            "{} : The {field} field is required.",
            error.path().join(".")
        );
    }
    error.to_string()
}
