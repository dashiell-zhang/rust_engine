/// 根据显式作用域与稳定类型标签生成方法标识
pub fn stable_method_key(
    scope: &str,

    self_type: Option<&str>,

    generic_types: &[String],

    const_generics: &[String],
) -> String {
    let mut key = format!("source-generator-method|scope={}:{}", scope.len(), scope);

    if let Some(self_type) = self_type {
        append_scoped_key_dimension(&mut key, "self", [self_type]);
    }

    append_scoped_key_dimension(
        &mut key,
        "generic",
        generic_types.iter().map(String::as_str),
    );

    append_scoped_key_dimension(&mut key, "const", const_generics.iter().map(String::as_str));

    key
}

/// 使用数量和长度前缀追加一组作用域键维度
pub(crate) fn append_scoped_key_dimension<'a>(
    key: &mut String,

    label: &str,

    values: impl IntoIterator<Item = &'a str>,
) {
    let values = values.into_iter().collect::<Vec<_>>();

    key.push('|');

    key.push_str(label);

    key.push('=');

    key.push_str(&values.len().to_string());

    for value in values {
        key.push(';');

        key.push_str(&value.len().to_string());

        key.push(':');

        key.push_str(value);
    }
}
