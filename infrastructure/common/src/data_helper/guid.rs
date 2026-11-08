use uuid::Uuid;

/// 数据导入、请求绑定和协议适配共用 Guid.Parse 的格式选择，不接受 urn:uuid 前缀。
pub fn parse(text: &str) -> Option<Uuid> {
    let text = text.trim();
    if text.encode_utf16().count() < 32 {
        return None;
    }
    match text.as_bytes().first()? {
        b'(' => parse_d(text.strip_prefix('(')?.strip_suffix(')')?),
        b'{' if text.encode_utf16().nth(9) == Some(u16::from(b'-')) => {
            parse_d(text.strip_prefix('{')?.strip_suffix('}')?)
        }
        b'{' => parse_x(text),
        _ if text.encode_utf16().nth(8) == Some(u16::from(b'-')) => parse_d(text),
        _ if text.len() == 32 && text.bytes().all(|byte| byte.is_ascii_hexdigit()) => {
            Uuid::parse_str(text).ok()
        }
        _ => None,
    }
}

fn parse_d(text: &str) -> Option<Uuid> {
    if !text.is_ascii()
        || text.len() != 36
        || [8, 13, 18, 23]
            .iter()
            .any(|&index| text.as_bytes()[index] != b'-')
    {
        return None;
    }
    if let Ok(value) = Uuid::parse_str(text) {
        return Some(value);
    }
    // 旧 Guid 的 D/B/P 分支允许部分分量带 +、0x，但分量总宽度不变。
    if !text.contains(['+', 'x', 'X']) {
        return None;
    }
    let a = component(&text[..8])?;
    let b = component(&text[9..13])? as u16;
    let c = component(&text[14..18])? as u16;
    let mut tail = [0; 8];
    tail[..2].copy_from_slice(&(component(&text[19..23])? as u16).to_be_bytes());
    tail[2..4].copy_from_slice(&(component(&text[24..28])? as u16).to_be_bytes());
    // 最后八位不使用上述前缀规则；源整数解析器允许末尾 NUL。
    let suffix = text[28..].trim_end_matches('\0');
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    tail[4..].copy_from_slice(&u32::from_str_radix(suffix, 16).ok()?.to_be_bytes());
    Some(from_parts(a, b, c, tail))
}

fn parse_x(text: &str) -> Option<Uuid> {
    // X 格式允许分量内部存在 Unicode 空白；其余格式只去除外部空白。
    let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    let body = compact.strip_prefix('{')?.strip_suffix('}')?;
    let mut parts = body.splitn(4, ',');
    let a = prefixed_component(parts.next()?)?;
    // 源实现按 UInt32 校验两个 short 分量，然后只保存低 16 位。
    let b = prefixed_component(parts.next()?)? as u16;
    let c = prefixed_component(parts.next()?)? as u16;
    let tail = parts.next()?.strip_prefix('{')?.strip_suffix('}')?;
    let mut parts = tail.split(',');
    let mut bytes = [0; 8];
    for byte in &mut bytes {
        *byte = prefixed_component(parts.next()?)?.try_into().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(from_parts(a, b, c, bytes))
}

fn prefixed_component(text: &str) -> Option<u32> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))?;
    if digits.is_empty() {
        return None;
    }
    component(digits)
}

fn component(text: &str) -> Option<u32> {
    let text = text.strip_prefix('+').unwrap_or(text);
    let text = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    // 保留源分量解析的零值和任意前导零规则，其他字符与 UInt32 溢出均拒绝。
    text.bytes().try_fold(0_u32, |value, digit| {
        let digit = match digit {
            b'0'..=b'9' => digit - b'0',
            b'a'..=b'f' => digit - b'a' + 10,
            b'A'..=b'F' => digit - b'A' + 10,
            _ => return None,
        };
        value.checked_mul(16)?.checked_add(u32::from(digit))
    })
}

fn from_parts(a: u32, b: u16, c: u16, tail: [u8; 8]) -> Uuid {
    let mut bytes = [0; 16];
    bytes[..4].copy_from_slice(&a.to_be_bytes());
    bytes[4..6].copy_from_slice(&b.to_be_bytes());
    bytes[6..8].copy_from_slice(&c.to_be_bytes());
    bytes[8..].copy_from_slice(&tail);
    Uuid::from_bytes(bytes)
}
