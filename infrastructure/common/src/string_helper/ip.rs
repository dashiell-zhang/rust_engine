use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

// 这些布尔查询只使用地址位，不使用 ScopeId；源解析也接受未知接口名并令 ScopeId 为 0。
pub(super) fn parse(text: &str) -> Option<IpAddr> {
    if text.contains(':') {
        parse_v6(text).map(IpAddr::V6)
    } else {
        parse_v4(text).map(IpAddr::V4)
    }
}

fn parse_v4(text: &str) -> Option<Ipv4Addr> {
    let mut parts = text.split('.').peekable();
    let mut address = 0u32;
    let mut count = 0u32;
    while let Some(part) = parts.next() {
        if count == 4 || part.is_empty() {
            return None;
        }
        let (digits, radix) =
            if let Some(digits) = part.strip_prefix("0x").or_else(|| part.strip_prefix("0X")) {
                (digits, 16)
            } else if part.starts_with('0') {
                (part, 8)
            } else {
                (part, 10)
            };
        if digits.is_empty() {
            return None;
        }
        let value = digits.bytes().try_fold(0u32, |value, byte| {
            value
                .checked_mul(radix)?
                .checked_add(char::from(byte).to_digit(radix)?)
        })?;
        if parts.peek().is_some() {
            if value > 255 || count == 3 {
                return None;
            }
            address |= value << (24 - count * 8);
        } else {
            // 最后一段可占剩余的 1 至 4 个字节。
            let maximum = u32::MAX >> (count * 8);
            if value > maximum {
                return None;
            }
            address |= value;
        }
        count += 1;
    }
    Some(Ipv4Addr::from(address))
}

fn parse_v6(text: &str) -> Option<Ipv6Addr> {
    let address = if let Some(bracketed) = text.strip_prefix('[') {
        let (address, trailing) = bracketed.split_once(']')?;
        if !trailing.is_empty() {
            let port = trailing.strip_prefix(':')?;
            // IPAddress 接受但忽略此端口；只检查字符，不限制数值范围，也允许空数字。
            let valid = if let Some(hex) = port.strip_prefix("0x") {
                hex.bytes().all(|byte| byte.is_ascii_hexdigit())
            } else {
                port.bytes().all(|byte| byte.is_ascii_digit())
            };
            if !valid {
                return None;
            }
        }
        address
    } else {
        if text.contains(']') {
            return None;
        }
        text
    };
    if address.contains('/') {
        return None;
    }
    let address = address.split_once('%').map_or(address, |(ip, _)| ip);
    if address.contains('.') {
        // 源 canonical IPv4 校验仅在分隔点检查前导零；末段允许单个前导零。
        let colon = address.rfind(':')?;
        let mut octets = [0u8; 4];
        let mut parts = address[colon + 1..].split('.');
        for (index, octet) in octets.iter_mut().enumerate() {
            let part = parts.next()?;
            if part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.starts_with("00")
                || (index < 3 && part.len() > 1 && part.starts_with('0'))
            {
                return None;
            }
            *octet = part.parse().ok()?;
        }
        if parts.next().is_some() {
            return None;
        }
        let [a, b, c, d] = octets;
        return format!("{}{a}.{b}.{c}.{d}", &address[..=colon])
            .parse()
            .ok();
    }
    address.parse().ok()
}

pub(super) fn parse_prefix(text: &str, maximum: u32) -> Option<u32> {
    // int.TryParse 的默认整数样式：ASCII 数字空白、正负号和末尾 NUL。
    let text = text
        .trim_end_matches('\0')
        .trim_matches(|ch| matches!(ch, ' ' | '\t'..='\r'));
    let negative = text.starts_with('-');
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = digits.parse::<u32>().ok()?;
    (value <= maximum && (!negative || value == 0)).then_some(value)
}
