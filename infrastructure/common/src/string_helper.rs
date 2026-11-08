use base64::{Engine as _, alphabet, engine::general_purpose};
use regex::Regex;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::LazyLock;
use uuid::Uuid;

mod collation;
mod compression;
mod html;
mod html_entities;
mod ip;
mod text;
mod unescape;
mod uri;
pub use collation::sort_current_culture_by;
pub use compression::{compress_string, decompress_string};
pub use html::remove_html;
pub use text::{
    decode_bom_text, partially_hidden, partially_hidden_utf16, substring_text, substring_text_utf16,
};
pub use unescape::{unicode_to_string, unicode_to_utf16};
pub use uri::model_to_uri_param;

// 源 char/Regex 按 UTF-16 字符判断；补充平面字符不能当作单个 BMP 字符分类。
static PUNCTUATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{P}&&[\x{0}-\x{FFFF}]]").expect("标点分类正则应有效"));
static DECIMAL_DIGITS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{Nd}&&[\x{0}-\x{FFFF}]]").expect("数字分类正则应有效"));

/// 过滤删除掉字符串中的全部标点符号
pub fn remove_punctuation(text: &str) -> String {
    PUNCTUATION.replace_all(text, "").into_owned()
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
    text.chars().all(is_cjk)
}

/// 判断字符串中是否全部为中文
pub fn is_all_cn(text: &str) -> bool {
    is_all_cjk(text)
}

/// 过滤删除掉字符串中的 Emoji 表情
pub fn remove_emoji(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            is_cjk(*ch) || ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '.' | '_' | '-')
        })
        .collect::<String>()
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

/// 过滤删除掉字符串中的数字
pub fn remove_number(text: &str) -> String {
    DECIMAL_DIGITS.replace_all(text, "").into_owned()
}

/// 过滤删除掉字符串中的非数字
pub fn remove_not_number(text: &str) -> String {
    DECIMAL_DIGITS
        .find_iter(text)
        .map(|value| value.as_str())
        .collect()
}

/// 压缩 GUID 到 22 位字符串，沿用 Guid.ToByteArray 的前三字段小端序。
pub fn compress_uuid(value: Uuid) -> String {
    let encoded = general_purpose::STANDARD.encode(value.to_bytes_le());
    encoded.replace('/', "_").replace('+', "-")[..22].to_string()
}

/// 与 NetEngine CompressGuid 相同的 22 位编码。
pub fn compress_guid(value: Uuid) -> String {
    compress_uuid(value)
}

/// 可失败的 GUID 解码；需要源 Guid.Empty/异常语义时使用 decompress_guid。
pub fn decompress_uuid(value: &str) -> Option<Uuid> {
    decode_short_guid(value).ok().flatten()
}

/// 无效长度或 Base64 格式返回空 GUID；合法 Base64 的字节数错误仍返回错误。
pub fn decompress_guid(value: &str) -> Result<Uuid, String> {
    decode_short_guid(value).map(|value| value.unwrap_or_else(Uuid::nil))
}

fn decode_short_guid(value: &str) -> Result<Option<Uuid>, String> {
    let value = value.trim();
    if value.encode_utf16().count() != 22 {
        return Ok(None);
    }
    let encoded = format!("{}==", value.replace('-', "+").replace('_', "/"));
    let Ok(bytes) = decode_base64(&encoded) else {
        return Ok(None);
    };
    let bytes: [u8; 16] = bytes
        .try_into()
        .map_err(|_| "GUID 字节长度必须为 16".to_owned())?;
    Ok(Some(Uuid::from_bytes_le(bytes)))
}

fn decode_base64(value: &str) -> Result<Vec<u8>, base64::DecodeError> {
    // Convert.FromBase64String 忽略四种 ASCII 空白，允许非零的未使用填充位。
    let encoded: Vec<_> = value
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .collect();
    let engine = general_purpose::GeneralPurpose::new(
        &alphabet::STANDARD,
        general_purpose::GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    );
    engine.decode(encoded)
}

/// 检查 IP 是否是局域网地址
pub fn is_lan_ip_address(ip_address: &str) -> bool {
    match ip::parse(ip_address) {
        Some(IpAddr::V4(ip)) => ip.is_private(),
        Some(IpAddr::V6(ip)) => ip.is_unique_local(),
        None => false,
    }
}

/// 检查 IPv4 地址是否是局域网 IP
pub fn is_lan_ip_address_v4(ip_address: &str) -> bool {
    matches!(ip::parse(ip_address), Some(IpAddr::V4(ip)) if ip.is_private())
}

/// 检查 IPv6 地址是否是局域网 IP
pub fn is_lan_ip_address_v6(ip_address: &str) -> bool {
    matches!(ip::parse(ip_address), Some(IpAddr::V6(ip)) if ip.is_unique_local())
}

/// 检查文本是否为可直接访问的公网 IP 地址
pub fn is_public_ip_address_text(ip_address: &str) -> bool {
    ip::parse(ip_address).is_some_and(is_public_ip_address)
}

/// 检查 IP 地址是否为可直接访问的公网地址
pub fn is_public_ip_address(ip_address: IpAddr) -> bool {
    match ip_address {
        IpAddr::V4(ipv4) => is_public_ipv4_address(ipv4),
        IpAddr::V6(ipv6) => {
            if let Some(ipv4) = ipv6.to_ipv4_mapped() {
                return is_public_ipv4_address(ipv4);
            }

            is_public_ipv6_address(ipv6)
        }
    }
}

/// 检查 IPv4 地址是否为公网地址
fn is_public_ipv4_address(ip_address: Ipv4Addr) -> bool {
    let [first, second, third, _] = ip_address.octets();

    match first {
        0 | 10 | 127 => false,
        100 if (64..=127).contains(&second) => false,
        169 if second == 254 => false,
        172 if (16..=31).contains(&second) => false,
        192 if second == 0 && matches!(third, 0 | 2) => false,
        192 if second == 88 && third == 99 => false,
        192 if second == 168 => false,
        198 if matches!(second, 18 | 19) => false,
        198 if second == 51 && third == 100 => false,
        203 if second == 0 && third == 113 => false,
        224..=255 => false,
        _ => true,
    }
}

/// 检查 IPv6 地址是否为公网地址
fn is_public_ipv6_address(ip_address: Ipv6Addr) -> bool {
    let bytes = ip_address.octets();

    // 仅允许当前全球单播地址空间 2000::/3
    if bytes[0] & 0xe0 != 0x20 {
        return false;
    }

    // 2001::/23 包含协议分配、基准测试和过渡地址
    if bytes[0] == 0x20 && bytes[1] == 0x01 && bytes[2] <= 0x01 {
        return false;
    }

    // 2001:db8::/32 为文档示例地址
    if bytes[0] == 0x20 && bytes[1] == 0x01 && bytes[2] == 0x0d && bytes[3] == 0xb8 {
        return false;
    }

    // 2002::/16 为 6to4 过渡地址
    if bytes[0] == 0x20 && bytes[1] == 0x02 {
        return false;
    }

    // 3ffe::/16 为已归还的旧 6bone 地址
    if bytes[0] == 0x3f && bytes[1] == 0xfe {
        return false;
    }

    // 3fff::/20 为文档示例地址
    if bytes[0] == 0x3f && bytes[1] == 0xff && bytes[2] & 0xf0 == 0 {
        return false;
    }

    true
}

/// 检查 IPv4 地址是否在指定 CIDR 范围内
pub fn is_ip_in_cidr_range_v4(ip_address: &str, cidr_range: &str) -> bool {
    let Some((network_text, prefix_text)) = cidr_range.split_once('/') else {
        return false;
    };

    let Some(IpAddr::V4(network)) = ip::parse(network_text) else {
        return false;
    };
    let Some(IpAddr::V4(ip)) = ip::parse(ip_address) else {
        return false;
    };
    let Some(prefix) = ip::parse_prefix(prefix_text, 32) else {
        return false;
    };

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

    let Some(IpAddr::V6(network)) = ip::parse(network_text) else {
        return false;
    };
    let Some(IpAddr::V6(ip)) = ip::parse(ip_address) else {
        return false;
    };
    let Some(prefix) = ip::parse_prefix(prefix_text, 128) else {
        return false;
    };

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
    if placeholder_prefix.is_empty() && placeholder_suffix.is_empty() {
        let value = placeholders.get("").ok_or("参数的值未找到")?;
        // 源空匹配按一个 UTF-16 单元向前推进，包括代理项对的中间位置。
        let replacement = value.encode_utf16().collect::<Vec<_>>();
        let mut output = replacement.clone();
        for unit in template.encode_utf16() {
            output.push(unit);
            output.extend_from_slice(&replacement);
        }
        return String::from_utf16(&output)
            .map_err(|_| "模板替换结果含有孤立 UTF-16 代理项".to_owned());
    }

    let pattern = format!(
        "{}(.*?){}",
        regex::escape(placeholder_prefix),
        regex::escape(placeholder_suffix)
    );
    let regex = Regex::new(&pattern).map_err(|error| error.to_string())?;

    let mut output = String::with_capacity(template.len());
    let mut offset = 0;
    for captures in regex.captures_iter(template) {
        let key = captures
            .get(1)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let value = placeholders
            .get(key)
            .ok_or_else(|| format!("{key}参数的值未找到"))?;
        let matched = captures.get(0).expect("参数匹配应包含整体范围");
        output.push_str(&template[offset..matched.start()]);
        output.push_str(value);
        offset = matched.end();
    }
    output.push_str(&template[offset..]);
    Ok(output)
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

/// 判断字符是否属于中日韩统一表意文字
fn is_cjk(value: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&value)
}

/// .NET OrdinalIgnoreCase 使用一对一大写映射，保留无点 i 和长 s。
/// https://github.com/dotnet/runtime/blob/main/src/native/libs/System.Globalization.Native/pal_common.c
pub fn eq_ordinal_ignore_case(left: &str, right: &str) -> bool {
    if left.is_ascii() && right.is_ascii() {
        return left.eq_ignore_ascii_case(right);
    }
    left.chars()
        .map(ordinal_upper)
        .eq(right.chars().map(ordinal_upper))
}

/// StringValues.ToString 的逗号合并规则：跳过空项，保留空白和原始顺序。
pub fn join_string_values<I, S>(values: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut joined = String::new();
    for value in values {
        append_string_value(&mut joined, value.as_ref());
    }
    joined
}

/// 向已经合并的 StringValues 文本追加一项，不引入空项的分隔符。
pub fn append_string_value(joined: &mut String, value: &str) {
    if !value.is_empty() {
        if !joined.is_empty() {
            joined.push(',');
        }
        joined.push_str(value);
    }
}

/// 用于具有 OrdinalIgnoreCase 语义的字典键，不修改原始文本值。
pub fn ordinal_ignore_case_key(value: &str) -> String {
    value.chars().map(ordinal_upper).collect()
}

fn ordinal_upper(ch: char) -> char {
    match ch {
        '\u{0131}' | '\u{017f}' => ch,
        _ => icu_casemap::CaseMapper::new().simple_uppercase(ch),
    }
}
