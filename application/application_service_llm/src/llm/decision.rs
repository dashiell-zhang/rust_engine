use application_model::llm::parameters::LlmParameters;
use common::app_error::AppError;
use common::json_helper::input::OrderedJson as Value;
use llm::decision::DecisionRequest;

use super::app_service::{placeholder_regex, render_template, validate_required_parameters};

pub(super) fn prepare(
    template: &str,
    system: Option<&str>,
    has_extra: bool,
    parameters: Option<&LlmParameters>,
) -> Result<(DecisionRequest, String), AppError> {
    if has_extra || system.is_some_and(|s| !s.trim().is_empty()) {
        return Err(AppError::bad_request(
            "请在决策请求模板中填写判断说明及扩展字段",
        ));
    }
    let mut root = common::json_helper::input::parse_unique_json(template)
        .map_err(|e| AppError::bad_request(format!("决策模板 JSON 格式错误: {e}")))?;
    let Value::Object(fields) = &mut root else {
        return Err(AppError::bad_request("决策请求模板必须是 JSON 对象"));
    };
    let removed = fields.shift_remove("model").is_some();
    if let Some(questions) = fields.get("questions").and_then(Value::as_object) {
        for (id, q) in questions {
            let mut types = q
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(k, _)| k.eq_ignore_ascii_case("type"))
                .map(|(_, v)| v.as_str());
            if !matches!(types.next(), Some(Some("choice" | "noul" | "score")))
                || types.next().is_some()
            {
                return Err(AppError::bad_request(format!(
                    "问题 {id} 的 type 必须是固定题型"
                )));
            }
        }
    }
    let saved = if removed && parameters.is_none() {
        serde_json::to_string_pretty(&root).map_err(|e| AppError::bad_request(e.to_string()))?
    } else {
        template.into()
    };
    visit_text(&mut root, parameters)?;
    let Value::Object(fields) = root else {
        unreachable!()
    };
    let request =
        DecisionRequest::from_template(fields).map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok((request, saved))
}

fn visit_text(value: &mut Value, params: Option<&LlmParameters>) -> Result<(), AppError> {
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                if placeholder_regex().is_match(key) {
                    return Err(AppError::bad_request("占位符不能用作字段名或问题标识"));
                }
                visit_text(child, params)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                visit_text(child, params)?;
            }
        }
        Value::String(s) => {
            if let Some(p) = params {
                validate_required_parameters(None, Some(s), p)?;
                *s = render_template(Some(s), p).unwrap_or_default();
            }
        }
        _ => (),
    }
    Ok(())
}
