use base64::{Engine as _, alphabet, engine::general_purpose};

/// HTTP/MIME Content-Disposition 参数，保留首个同名参数及原始名称。
/// 编码文件名优先于普通文件名，由调用方决定空值与路径的语义。
pub struct ContentDisposition {
    pub disposition_type: String,
    pub name: Option<String>,
    pub filename_star: Option<String>,
    pub filename: Option<String>,
}

/// 解析含引号分号的参数，保留源 ContentDisposition 属性不解除反斜杠的行为。
pub fn parse_content_disposition(header: &str) -> Option<ContentDisposition> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in header.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
        }
        if ch == ';' && !quoted {
            parts.push(header[start..index].trim());
            start = index + 1;
        }
    }
    if quoted || escaped {
        return None;
    }
    parts.push(header[start..].trim());
    if parts.first().is_none_or(|part| !is_token(part)) {
        return None;
    }
    let disposition_type = parts[0].to_string();
    let mut name = None;
    let mut filename = None;
    let mut filename_star = None;
    let mut star_seen = false;
    for part in parts.into_iter().skip(1) {
        let (key, value) = part.split_once('=')?;
        let key = key.trim();
        let value = value.trim();
        let quoted = value.starts_with('"') && value.ends_with('"') && value.len() >= 2;
        if !is_token(key) || (!quoted && !is_token(value)) {
            return None;
        }
        if key.eq_ignore_ascii_case("filename*") && !star_seen {
            star_seen = true;
            let values: Vec<_> = value.split('\'').collect();
            if values.len() == 3 {
                filename_star =
                    decode_charset(values[0], &urlencoding::decode_binary(values[2].as_bytes()));
            }
        } else if (key.eq_ignore_ascii_case("filename") && filename.is_none())
            || (key.eq_ignore_ascii_case("name") && name.is_none())
        {
            let unquoted = if quoted {
                &value[1..value.len() - 1]
            } else {
                value
            };
            let fields: Vec<_> = unquoted.split('?').collect();
            let decoded = if quoted
                && fields.len() == 5
                && fields[0] == "="
                && fields[4] == "="
                && fields[2].eq_ignore_ascii_case("B")
            {
                let encoded: Vec<_> = fields[3]
                    .bytes()
                    .filter(|value| !matches!(value, b' ' | b'\t' | b'\r' | b'\n'))
                    .collect();
                general_purpose::GeneralPurpose::new(
                    &alphabet::STANDARD,
                    general_purpose::GeneralPurposeConfig::new()
                        .with_decode_allow_trailing_bits(true),
                )
                .decode(encoded)
                .ok()
                .and_then(|data| decode_charset(fields[1], &data))
            } else {
                None
            };
            let value = Some(decoded.unwrap_or_else(|| unquoted.into()));
            if key.eq_ignore_ascii_case("name") {
                name = value;
            } else {
                filename = value;
            }
        }
    }
    Some(ContentDisposition {
        disposition_type,
        name,
        filename_star,
        filename,
    })
}

fn is_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

/// HTTP 下载文件名常用编码；无法识别的 filename* 回退普通文件名。
fn decode_charset(encoding: &str, data: &[u8]) -> Option<String> {
    match encoding.to_ascii_lowercase().as_str() {
        "utf-8" | "utf8" => Some(String::from_utf8_lossy(data).into_owned()),
        "us-ascii" | "ascii" => Some(
            data.iter()
                .map(|byte| {
                    if byte.is_ascii() {
                        char::from(*byte)
                    } else {
                        '?'
                    }
                })
                .collect(),
        ),
        "iso-8859-1" | "latin1" => Some(data.iter().map(|byte| char::from(*byte)).collect()),
        "utf-16" | "utf-16le" | "unicode" => Some(decode_utf16(data, false)),
        "utf-16be" => Some(decode_utf16(data, true)),
        _ => None,
    }
}

fn decode_utf16(data: &[u8], big_endian: bool) -> String {
    let (pairs, remainder) = data.as_chunks::<2>();
    let units: Vec<_> = pairs
        .iter()
        .map(|chunk| {
            if big_endian {
                u16::from_be_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_le_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    let mut value = String::from_utf16_lossy(&units);
    if !remainder.is_empty() {
        value.push(char::REPLACEMENT_CHARACTER);
    }
    value
}
