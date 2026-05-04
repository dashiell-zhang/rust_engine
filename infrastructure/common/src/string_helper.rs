use base64::{Engine as _, engine::general_purpose};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use regex::Regex;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use uuid::Uuid;

/// 过滤删除掉字符串中的全部标点符号
pub fn remove_punctuation(text: &str) -> String {
    text.chars()
        .filter(|value| !value.is_ascii_punctuation() && !is_unicode_punctuation(*value))
        .collect()
}

/// 判断字符串中是否包含中文
pub fn contains_cjk(text: &str) -> bool {
    text.chars().any(is_cjk)
}

/// 判断字符串中是否包含中文
pub fn is_contains_cn(text: &str) -> bool {
    contains_cjk(text)
}

/// 判断字符串中是否全部为中文
pub fn is_all_cjk(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_cjk)
}

/// 判断字符串中是否全部为中文
pub fn is_all_cn(text: &str) -> bool {
    is_all_cjk(text)
}

/// 过滤删除掉字符串中的 HTML 标签
pub fn remove_html(html_text: &str) -> String {
    let script_regex = Regex::new(r"(?is)<script[^>]*?>.*?</script>").expect("脚本标签正则应有效");
    let tag_regex = Regex::new(r"(?is)<[^>]+>").expect("HTML 标签正则应有效");
    let space_regex = Regex::new(r"\s+").expect("空白正则应有效");

    let text = script_regex.replace_all(html_text, "");
    let text = tag_regex.replace_all(&text, "");
    let text = html_unescape(text.as_ref());

    space_regex.replace_all(text.trim(), " ").into_owned()
}

/// 过滤删除掉字符串中的 Emoji 表情
pub fn remove_emoji(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !is_emoji(*ch))
        .collect::<String>()
}

/// 对文本进行指定长度截取并添加省略号
pub fn substring_text(text: &str, length: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= length {
        return text.to_string();
    }

    chars.into_iter().take(length).collect::<String>() + "..."
}

/// 对字符串进行局部隐藏处理
pub fn partially_hidden(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return String::new();
    }

    let total = chars.len();
    let start = if total >= 3 {
        total / 3
    } else {
        1.min(total - 1)
    };
    let hidden_count = if total >= 3 { total / 3 } else { 1.min(total) };
    let end = (start + hidden_count).min(total);

    chars
        .into_iter()
        .enumerate()
        .map(|(index, ch)| {
            if index >= start && index < end {
                '*'
            } else {
                ch
            }
        })
        .collect()
}

/// 规范化可选文本
pub fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|item| normalize_optional_str(&item))
}

/// 规范化字符串切片并转为可选拥有值
pub fn normalize_optional_str(value: &str) -> Option<String> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Unicode 转普通字符串
pub fn unicode_to_string(text: &str) -> Result<String, serde_json::Error> {
    serde_json::from_str::<String>(&format!("\"{}\"", escape_json_text_preserve_slash(text)))
}

/// 过滤删除掉字符串中的数字
pub fn remove_number(text: &str) -> String {
    text.chars()
        .filter(|value| !value.is_ascii_digit())
        .collect()
}

/// 过滤删除掉字符串中的非数字
pub fn remove_not_number(text: &str) -> String {
    text.chars()
        .filter(|value| value.is_ascii_digit())
        .collect()
}

/// 字符串压缩
pub fn compress_string(text: &str) -> Result<String, std::io::Error> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(text.as_bytes())?;
    let compressed = encoder.finish()?;

    Ok(general_purpose::STANDARD.encode(compressed))
}

/// 字符串解压
pub fn decompress_string(text: &str) -> Result<String, std::io::Error> {
    let compressed = general_purpose::STANDARD
        .decode(text)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut output = String::new();
    decoder.read_to_string(&mut output)?;

    Ok(output)
}

/// 压缩 UUID 到 22 位字符串
pub fn compress_uuid(value: Uuid) -> String {
    let encoded = general_purpose::STANDARD.encode(value.as_bytes());
    encoded.replace('/', "_").replace('+', "-")[..22].to_string()
}

/// 压缩 UUID 到 22 位字符串
pub fn compress_guid(value: Uuid) -> String {
    compress_uuid(value)
}

/// 将 22 位字符串还原为 UUID
pub fn decompress_uuid(value: &str) -> Option<Uuid> {
    if value.trim().len() != 22 {
        return None;
    }

    let encoded = format!("{}==", value.trim().replace('-', "+").replace('_', "/"));
    let bytes = general_purpose::STANDARD.decode(encoded).ok()?;

    Uuid::from_slice(&bytes).ok()
}

/// 将 22 位字符串还原为 UUID
pub fn decompress_guid(value: &str) -> Option<Uuid> {
    decompress_uuid(value)
}

/// 检查 IP 是否是局域网地址
pub fn is_lan_ip_address(ip_address: &str) -> bool {
    match ip_address.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_broadcast()
        }
        Ok(IpAddr::V6(ip)) => {
            ip.is_unique_local() || ip.is_loopback() || ip.is_unicast_link_local()
        }
        Err(_) => false,
    }
}

/// 检查 IPv4 地址是否是局域网 IP
pub fn is_lan_ip_address_v4(ip_address: &str) -> bool {
    match ip_address.parse::<Ipv4Addr>() {
        Ok(ip) => {
            ip.octets()[0] == 10
                || (ip.octets()[0] == 172 && (16..=31).contains(&ip.octets()[1]))
                || (ip.octets()[0] == 192 && ip.octets()[1] == 168)
        }
        Err(_) => false,
    }
}

/// 检查 IPv6 地址是否是局域网 IP
pub fn is_lan_ip_address_v6(ip_address: &str) -> bool {
    match ip_address.parse::<Ipv6Addr>() {
        Ok(ip) => ip.is_unique_local(),
        Err(_) => false,
    }
}

/// 检查 IPv4 地址是否在指定 CIDR 范围内
pub fn is_ip_in_cidr_range_v4(ip_address: &str, cidr_range: &str) -> bool {
    let Some((network_text, prefix_text)) = cidr_range.split_once('/') else {
        return false;
    };

    let Ok(network) = network_text.parse::<Ipv4Addr>() else {
        return false;
    };
    let Ok(ip) = ip_address.parse::<Ipv4Addr>() else {
        return false;
    };
    let Ok(prefix) = prefix_text.parse::<u32>() else {
        return false;
    };

    if prefix > 32 {
        return false;
    }

    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };

    (u32::from(ip) & mask) == (u32::from(network) & mask)
}

/// 检查 IPv6 地址是否在指定 CIDR 范围内
pub fn is_ip_in_cidr_range_v6(ip_address: &str, cidr_range: &str) -> bool {
    let Some((network_text, prefix_text)) = cidr_range.split_once('/') else {
        return false;
    };

    let Ok(network) = network_text.parse::<Ipv6Addr>() else {
        return false;
    };
    let Ok(ip) = ip_address.parse::<Ipv6Addr>() else {
        return false;
    };
    let Ok(prefix) = prefix_text.parse::<u32>() else {
        return false;
    };

    if prefix > 128 {
        return false;
    }

    let network = u128::from_be_bytes(network.octets());
    let ip = u128::from_be_bytes(ip.octets());
    let shift = 128 - prefix;

    if prefix == 0 {
        return true;
    }

    (ip >> shift) == (network >> shift)
}

/// 替换模板字符串中的参数
pub fn replace_placeholders(
    template: &str,
    placeholders: &HashMap<String, String>,
    placeholder_prefix: &str,
    placeholder_suffix: &str,
) -> Result<String, String> {
    let pattern = format!(
        "{}(.*?){}",
        regex::escape(placeholder_prefix),
        regex::escape(placeholder_suffix)
    );
    let regex = Regex::new(&pattern).map_err(|error| error.to_string())?;

    let mut missing_key = None;
    let result = regex.replace_all(template, |captures: &regex::Captures<'_>| {
        let key = captures
            .get(1)
            .map(|value| value.as_str())
            .unwrap_or_default();
        if let Some(value) = placeholders.get(key) {
            value.clone()
        } else {
            missing_key = Some(key.to_string());
            captures
                .get(0)
                .map(|value| value.as_str())
                .unwrap_or_default()
                .to_string()
        }
    });

    if let Some(key) = missing_key {
        return Err(format!("{key} 参数的值未找到"));
    }

    Ok(result.into_owned())
}

/// 去掉名称中的父级前缀
pub fn trim_prefix_name(name: &str, parent_name: Option<&str>) -> String {
    match parent_name {
        Some(parent) => name
            .strip_prefix(&format!("{parent}-"))
            .unwrap_or(name)
            .to_string(),
        None => name.to_string(),
    }
}

/// 将模型对象转换为 Uri 参数字符串
pub fn model_to_uri_param<T>(value: &T, url: &str) -> Result<String, serde_json::Error>
where
    T: serde::Serialize,
{
    let object = serde_json::to_value(value)?;
    let mut result = String::from(url);
    let mut first = true;

    if let serde_json::Value::Object(map) = object {
        for (key, value) in map {
            if value.is_null() {
                continue;
            }

            let text = match value {
                serde_json::Value::String(item) if item.trim().is_empty() => continue,
                serde_json::Value::String(item) => item,
                serde_json::Value::Number(item) => item.to_string(),
                serde_json::Value::Bool(item) => item.to_string(),
                other => other.to_string(),
            };

            if first {
                if !url.ends_with('&') && !url.ends_with('?') {
                    if url.contains('?') {
                        result.push('&');
                    } else {
                        result.push('?');
                    }
                }
            } else {
                result.push('&');
            }

            first = false;
            result.push_str(&urlencoding::encode(&key));
            result.push('=');
            result.push_str(&urlencoding::encode(&text));
        }
    }

    Ok(result)
}

/// 判断字符是否属于中日韩统一表意文字
fn is_cjk(value: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&value)
}

/// 判断字符是否属于 Unicode 标点符号
fn is_unicode_punctuation(value: char) -> bool {
    matches!(
        value as u32,
        0x2000..=0x206F
            | 0x2E00..=0x2E7F
            | 0x3000..=0x303F
            | 0xFF00..=0xFF0F
            | 0xFF1A..=0xFF20
            | 0xFF3B..=0xFF40
            | 0xFF5B..=0xFF65
    )
}

/// 判断字符是否属于常见 Emoji 区间
fn is_emoji(value: char) -> bool {
    matches!(
        value as u32,
        0x1F300..=0x1FAFF | 0x2600..=0x27BF | 0xFE00..=0xFE0F | 0x1F1E6..=0x1F1FF
    )
}

/// 解码常见 HTML 实体
fn html_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&amp;", "&")
        .replace("&#38;", "&")
        .replace("&lt;", "<")
        .replace("&#60;", "<")
        .replace("&gt;", ">")
        .replace("&#62;", ">")
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&iexcl;", "\u{00A1}")
        .replace("&cent;", "\u{00A2}")
        .replace("&pound;", "\u{00A3}")
        .replace("&copy;", "\u{00A9}")
}

/// 转义 JSON 字符串内容，同时保留反斜杠转义序列供 serde_json 解码
fn escape_json_text_preserve_slash(value: &str) -> String {
    let mut output = String::with_capacity(value.len());

    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0C}' => output.push_str("\\f"),
            ch if ch.is_control() => output.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => output.push(ch),
        }
    }

    output
}
