mod budget;
mod scanner;
mod serializer;
mod writer;

pub(crate) use budget::*;
pub(crate) use scanner::*;
pub(crate) use serializer::*;
pub(crate) use writer::*;

use crate::*;

/// 递归规范化 JSON 对象字段并限制输入复杂度
pub(crate) fn canonicalize_json(
    value: serde_json::Value,
    depth: usize,
    item_count: &mut usize,
    max_depth: usize,
    max_items: usize,
) -> Result<serde_json::Value, InvocationKeyError> {
    check_json_limits(depth, item_count, max_depth, max_items)?;
    match value {
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(|value| canonicalize_json(value, depth + 1, item_count, max_depth, max_items))
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        serde_json::Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));

            entries
                .into_iter()
                .map(|(key, value)| {
                    canonicalize_json(value, depth + 1, item_count, max_depth, max_items)
                        .map(|value| (key, value))
                })
                .collect::<Result<serde_json::Map<_, _>, _>>()
                .map(serde_json::Value::Object)
        }
        value => Ok(value),
    }
}

/// 检查当前 JSON 节点是否超过深度或项目数限制
pub(crate) fn check_json_limits(
    depth: usize,
    item_count: &mut usize,
    max_depth: usize,
    max_items: usize,
) -> Result<(), InvocationKeyError> {
    if depth > max_depth {
        return Err(InvocationKeyError::new(format!(
            "参数 JSON 超过 {max_depth} 层深度限制"
        )));
    }

    *item_count = item_count
        .checked_add(1)
        .ok_or_else(|| InvocationKeyError::new("参数 JSON 项目计数溢出"))?;

    if *item_count > max_items {
        return Err(InvocationKeyError::new(format!(
            "参数 JSON 超过 {max_items} 项限制"
        )));
    }

    Ok(())
}
