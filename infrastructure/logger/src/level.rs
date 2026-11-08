/// 对齐 LogLevel 的 Int32 枚举解析，保留 Rust 配置已有的简写。
pub(super) fn parse(text: &str) -> Result<Option<i32>, ()> {
    // 配置中的空字符串不产生规则；仅含空白的字符串是非法枚举。
    if text.is_empty() {
        return Ok(None);
    }
    let text = text.trim();
    if text.is_empty() {
        return Err(());
    }
    if matches!(text.as_bytes()[0], b'+' | b'-' | b'0'..=b'9') {
        return text.parse::<i32>().map(Some).map_err(|_| ());
    }
    // Enum.TryParse 接受以逗号组合的名称，即使枚举没有 Flags 标记。
    let mut value = 0;
    for name in text.split(',') {
        value |= match name.trim().to_ascii_lowercase().as_str() {
            "trace" => 0,
            "debug" => 1,
            "info" | "information" => 2,
            "warn" | "warning" => 3,
            "error" => 4,
            "critical" => 5,
            "none" | "off" => 6,
            _ => return Err(()),
        };
    }
    Ok(Some(value))
}
