use crate::*;

/// 生成 cacheable 行为使用的缓存键
pub fn cacheable_key(method_key: &str, arguments_key: &str) -> String {
    format!(
        "CacheData_{}",
        compose_invocation_hash(method_key, Some(arguments_key))
    )
}

/// 生成 cacheable 行为使用的防击穿锁键
pub fn cacheable_lock_key(method_key: &str, arguments_key: &str) -> String {
    format!(
        "CacheDataLock_{}",
        compose_invocation_hash(method_key, Some(arguments_key))
    )
}

/// 生成 concurrency_limit 行为使用的锁键
pub fn concurrency_limit_key(method_key: &str, arguments_key: Option<&str>) -> String {
    format!(
        "ConcurrencyLimit_{}",
        compose_invocation_hash(method_key, arguments_key)
    )
}

/// 计算方法标识与可选参数快照组成的调用摘要
pub(crate) fn compose_invocation_hash(method_key: &str, arguments_key: Option<&str>) -> String {
    if method_key.chars().any(char::is_control) {
        return compose_structured_invocation_hash(method_key, arguments_key);
    }

    let mut seed = method_key.to_string();

    if let Some(arguments_key) = arguments_key {
        seed.push('\n');
        seed.push_str(arguments_key);
    }

    sha256_hex(&seed)
}

/// 对包含控制字符的方法键使用无歧义的长度前缀编码
pub(crate) fn compose_structured_invocation_hash(
    method_key: &str,
    arguments_key: Option<&str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"source_generator_invocation_key\0");
    hasher.update(
        u64::try_from(method_key.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    hasher.update(method_key.as_bytes());

    if let Some(arguments_key) = arguments_key {
        hasher.update([1]);
        hasher.update(
            u64::try_from(arguments_key.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        hasher.update(arguments_key.as_bytes());
    } else {
        hasher.update([0]);
    }

    let hash = hasher.finalize();
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 计算输入字符串的 SHA-256 十六进制摘要
pub(crate) fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();

    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}
