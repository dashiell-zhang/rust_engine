//! 提供商状态检查及非流式响应读取；字符集来自 Content-Type，不猜测未声明的编码。
use crate::LlmError;
use serde::de::DeserializeOwned;
use std::borrow::Cow;

/// 与源 EnsureSuccessStatusCode 一致，仅接受最终 2xx 响应。
pub(crate) fn ensure_success(response: &reqwest::Response) -> Result<(), LlmError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let reason = response
        .extensions()
        .get::<hyper::ext::ReasonPhrase>()
        .map(|reason| {
            reason
                .as_bytes()
                .iter()
                .map(|byte| char::from(*byte))
                .collect::<String>()
        })
        .unwrap_or_else(|| status.canonical_reason().unwrap_or_default().to_owned());
    let code = status.as_u16();
    let message = if reason.trim().is_empty() {
        format!("Response status code does not indicate success: {code}.")
    } else {
        format!("Response status code does not indicate success: {code} ({reason}).")
    };
    Err(LlmError::ApiError {
        status: code,
        message,
    })
}

pub(crate) async fn read_json<T: DeserializeOwned>(
    response: reqwest::Response,
    empty_message: &str,
) -> Result<T, LlmError> {
    let status_result = ensure_success(&response);
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<mime::Mime>().ok());
    let charset = content_type
        .as_ref()
        .and_then(|value| value.get_param(mime::CHARSET));
    // 非流式源调用先缓冲正文，再检查状态；保留传输失败/超时先于状态和字符集错误。
    let bytes = response.bytes().await?;
    status_result?;
    let encoding = Encoding::parse(charset.as_ref().map(|value| value.as_str()))?;
    let json = encoding.decode(&bytes);
    // JsonSerializer.DeserializeAsync 接受流开头的一个 UTF-8 BOM；先转码再处理前导标记。
    let json = json.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&json);
    crate::wire::from_slice::<Option<T>>(json)?
        .ok_or_else(|| LlmError::InvalidResponse(empty_message.to_owned()))
}

enum Encoding {
    Utf8,
    Utf16 { big_endian: bool },
    Utf32 { big_endian: bool },
    Ascii,
    Latin1,
}

impl Encoding {
    fn parse(charset: Option<&str>) -> Result<Self, LlmError> {
        // 对应未注册额外代码页提供程序的 .NET Encoding 内置名称。
        let Some(charset) = charset else {
            return Ok(Self::Utf8);
        };
        Ok(match charset.to_ascii_lowercase().as_str() {
            "utf-7"
            | "csunicode11utf7"
            | "unicode-1-1-utf-7"
            | "unicode-2-0-utf-7"
            | "x-unicode-1-1-utf-7"
            | "x-unicode-2-0-utf-7" => {
                return Err(LlmError::InvalidResponse(
                    "Support for UTF-7 is disabled. See https://aka.ms/dotnet-warnings/SYSLIB0001 for more information.".into(),
                ));
            }
            "utf-8"
            | "unicode-1-1-utf-8"
            | "unicode-2-0-utf-8"
            | "x-unicode-1-1-utf-8"
            | "x-unicode-2-0-utf-8" => Self::Utf8,
            "utf-16" | "utf-16le" | "unicode" | "ucs-2" | "iso-10646-ucs-2" => {
                Self::Utf16 { big_endian: false }
            }
            "utf-16be" | "unicodefffe" => Self::Utf16 { big_endian: true },
            "utf-32" | "utf-32le" => Self::Utf32 { big_endian: false },
            "utf-32be" => Self::Utf32 { big_endian: true },
            "us-ascii" | "ascii" | "us" | "ansi_x3.4-1968" | "ansi_x3.4-1986" | "cp367"
            | "csascii" | "ibm367" | "iso-ir-6" | "iso646-us" | "iso_646.irv:1991" => Self::Ascii,
            "iso-8859-1" | "iso8859-1" | "iso_8859-1" | "iso_8859-1:1987" | "iso-ir-100"
            | "latin1" | "l1" | "cp819" | "csisolatin1" | "ibm819" => Self::Latin1,
            _ => {
                return Err(LlmError::InvalidResponse(
                    "The character set provided in ContentType is invalid.".into(),
                ));
            }
        })
    }

    fn decode<'a>(&self, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        let text = match self {
            // UTF-8 保留原始字节，由 JSON 读取器处理非法序列，不做有损替换。
            Self::Utf8 => return Cow::Borrowed(bytes),
            Self::Ascii => bytes
                .iter()
                .map(|byte| {
                    if byte.is_ascii() {
                        char::from(*byte)
                    } else {
                        '?'
                    }
                })
                .collect(),
            Self::Latin1 => bytes.iter().map(|byte| char::from(*byte)).collect(),
            Self::Utf16 { big_endian } => {
                let (pairs, remainder) = bytes.as_chunks::<2>();
                let words = pairs.iter().map(|pair| {
                    if *big_endian {
                        u16::from_be_bytes(*pair)
                    } else {
                        u16::from_le_bytes(*pair)
                    }
                });
                let mut text: String = char::decode_utf16(words)
                    .map(|value| value.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect();
                if !remainder.is_empty() {
                    text.push(char::REPLACEMENT_CHARACTER);
                }
                text
            }
            Self::Utf32 { big_endian } => {
                let (words, remainder) = bytes.as_chunks::<4>();
                let mut text: String = words
                    .iter()
                    .map(|word| {
                        let value = if *big_endian {
                            u32::from_be_bytes(*word)
                        } else {
                            u32::from_le_bytes(*word)
                        };
                        char::from_u32(value).unwrap_or(char::REPLACEMENT_CHARACTER)
                    })
                    .collect();
                if !remainder.is_empty() {
                    text.push(char::REPLACEMENT_CHARACTER);
                }
                text
            }
        };
        Cow::Owned(text.into_bytes())
    }
}
