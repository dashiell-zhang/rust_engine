//! 正文参数身份和缺失值诊断归属 HTTP 绑定层，不放进可复用 DTO。
use std::borrow::Cow;

use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
    http::{Version, header},
    response::{IntoResponse, Response},
};
use common::json_helper::input::{JsonModel, JsonReadError, read_json_model};
use http_body::Body as _;

#[derive(Clone, Copy)]
pub(crate) struct Parameter(pub &'static str);

/// 在应用中间件等待或包装正文前保留传输层的正文存在标记。
#[derive(Clone, Copy)]
pub(crate) struct BodyPresence(pub bool);

impl BodyPresence {
    pub(crate) fn from_request(request: &Request) -> Self {
        // HTTP/2 的 is_end_stream 会随接收进度变化，不能在鉴权之后覆盖入口快照。
        if let Some(presence) = request.extensions().get::<Self>() {
            return *presence;
        }
        Self(if request.version() == Version::HTTP_2 {
            !request.body().is_end_stream()
        } else {
            request.headers().contains_key(header::TRANSFER_ENCODING)
                || request
                    .headers()
                    .get(header::CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .is_some_and(|length| length > 0)
        })
    }
}

/// 一次读取原文，一次绑定模型；同时供普通 JSON 和需保留签名原文的回调使用。
pub(crate) async fn read<T: JsonModel, S: Send + Sync>(
    request: Request,
    state: &S,
) -> Result<(T, Bytes), Box<Response>> {
    let parameter = request.extensions().get::<Parameter>().copied();
    let has_body = request
        .extensions()
        .get::<BodyPresence>()
        .copied()
        .unwrap_or_else(|| BodyPresence::from_request(&request))
        .0;
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let media_type = content_type
        .and_then(|value| value.split(';').next())
        .and_then(|value| value.trim().parse::<mime::Mime>().ok());
    let accepted = media_type.as_ref().is_some_and(|media_type| {
        (media_type.type_() == mime::APPLICATION
            && (media_type.subtype() == mime::JSON || media_type.suffix() == Some(mime::JSON)))
            || (media_type.type_() == mime::TEXT && media_type.subtype() == mime::JSON)
    });
    if !accepted {
        return Err(Box::new(crate::problem_details::unsupported_media_type(
            request.headers(),
        )));
    }
    // 源 formatter 先判断传输层是否有正文，再选择编码；空 chunked 流仍需解析。
    if !has_body {
        return Err(missing(parameter));
    }
    let encoding = charset(content_type.unwrap_or_default())
        .map_err(|()| {
            Box::new(crate::error::internal_error_response(
                "JSON Content-Type 参数不完整",
            ))
        })?
        .map(str::to_ascii_lowercase);
    let utf16 = match encoding.as_deref() {
        None
        | Some(
            "utf-8"
            | "unicode-1-1-utf-8"
            | "unicode-2-0-utf-8"
            | "x-unicode-1-1-utf-8"
            | "x-unicode-2-0-utf-8",
        ) => false,
        Some("utf-16" | "utf-16le" | "unicode" | "ucs-2" | "iso-10646-ucs-2") => true,
        Some(_) => {
            return Err(Box::new(crate::problem_details::unsupported_media_type(
                request.headers(),
            )));
        }
    };
    let bytes = Bytes::from_request(request, state)
        .await
        .map_err(|error| Box::new(error.into_response()))?;
    let decoded = if utf16 {
        Cow::Owned(decode_utf16(&bytes).map_err(|()| {
            Box::new(crate::error::internal_error_response(
                "JSON UTF-16 正文解码失败",
            ))
        })?)
    } else {
        Cow::Borrowed(bytes.as_ref())
    };
    // 转码及 BOM 处理只作用于 JSON 读取视图，回调和签名仍读取原始字节。
    let json_bytes = decoded.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&decoded);
    let parsed = read_json_model::<T>(json_bytes);
    match parsed {
        Ok(Some(value)) => Ok((value, bytes)),
        Ok(None) => Err(missing(parameter)),
        Err(JsonReadError::Depth {
            maximum,
            container,
            path,
            line,
            column,
        }) => Err(error(
            parameter,
            &path,
            format!(
                "The maximum configured depth of {maximum} has been exceeded. Cannot read next JSON {container}. Path: {path} | LineNumber: {line} | BytePositionInLine: {column}."
            ),
        )),
        Err(JsonReadError::Shape {
            type_name,
            path,
            line,
            column,
        }) => Err(error(
            parameter,
            &path,
            format!(
                "The JSON value could not be converted to {type_name}. Path: {path} | LineNumber: {line} | BytePositionInLine: {column}."
            ),
        )),
        Err(JsonReadError::Validation(errors)) => Err(Box::new(
            crate::request_binding::model_errors(&errors).into_response(),
        )),
        Err(JsonReadError::UnterminatedString { path, line, column }) => Err(error(
            parameter,
            &path,
            format!(
                "Expected end of string, but instead reached end of data. Path: {path} | LineNumber: {line} | BytePositionInLine: {column}."
            ),
        )),
        Err(JsonReadError::EndAfterSeparator { path, line, column }) => Err(error(
            parameter,
            &path,
            format!(
                "Expected start of a property name or value, but instead reached end of data. Path: {path} | LineNumber: {line} | BytePositionInLine: {column}."
            ),
        )),
        Err(JsonReadError::Trailing { byte, line, column }) => {
            let character = if (b' '..=b'~').contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("0x{byte:02X}")
            };
            Err(error(
                parameter,
                "$",
                format!(
                    "'{character}' is invalid after a single JSON value. Expected end of data. Path: $ | LineNumber: {line} | BytePositionInLine: {column}."
                ),
            ))
        }
        Err(JsonReadError::Syntax { .. })
            if json_bytes
                .iter()
                .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n')) =>
        {
            let line = json_bytes.iter().filter(|byte| **byte == b'\n').count();
            let column = json_bytes
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map(|index| json_bytes.len() - index - 1)
                .unwrap_or(json_bytes.len());
            Err(error(
                parameter,
                "$",
                format!(
                    "The input does not contain any JSON tokens. Expected the input to start with a valid JSON token, when isFinalBlock is true. Path: $ | LineNumber: {line} | BytePositionInLine: {column}."
                ),
            ))
        }
        Err(JsonReadError::Syntax {
            error: syntax,
            path,
        }) => Err(error(
            parameter,
            &path,
            format!("Failed to parse the request body as JSON: {syntax}"),
        )),
        Err(JsonReadError::Conversion(error)) => Err(Box::new(
            crate::request_binding::ModelErrors(vec![(
                format!("$.{}", error.path().join(".")),
                format!("Failed to deserialize the JSON body into the target type: {error}"),
            )])
            .into_response(),
        )),
    }
}

fn decode_utf16(bytes: &[u8]) -> Result<Vec<u8>, ()> {
    let (pairs, remainder) = bytes.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(());
    }
    let units = pairs.iter().copied().map(u16::from_le_bytes);
    let mut text = String::with_capacity(bytes.len());
    for character in char::decode_utf16(units) {
        text.push(character.map_err(|_| ())?);
    }
    Ok(text.into_bytes())
}

/// 源媒体类型读取器按顺序取第一个 charset；遇到无效参数便停止，不否定已匹配的媒体类型。
fn charset(content_type: &str) -> Result<Option<&str>, ()> {
    let Some((_, mut rest)) = content_type.split_once(';') else {
        return Ok(None);
    };
    loop {
        rest = rest.trim_start_matches([' ', '\t']);
        let name_length = rest.bytes().take_while(|byte| token(*byte)).count();
        if name_length == 0 {
            return Ok(None);
        }
        let (name, tail) = rest.split_at(name_length);
        let Some(tail) = tail.trim_start_matches([' ', '\t']).strip_prefix('=') else {
            return Ok(None);
        };
        rest = tail.trim_start_matches([' ', '\t']);
        if rest.is_empty() {
            return Err(());
        }
        let (value, tail) = if let Some(quoted) = rest.strip_prefix('"') {
            let mut escaped = false;
            let end = quoted.bytes().position(|byte| {
                if escaped {
                    escaped = false;
                    false
                } else if byte == b'\\' {
                    escaped = true;
                    false
                } else {
                    byte == b'"'
                }
            });
            let Some(end) = end else { return Ok(None) };
            (&quoted[..end], &quoted[end + 1..])
        } else {
            let length = rest.bytes().take_while(|byte| token(*byte)).count();
            if length == 0 {
                return Ok(None);
            }
            rest.split_at(length)
        };
        if name.eq_ignore_ascii_case("charset") {
            return Ok(Some(value));
        }
        let Some(tail) = tail.trim_start_matches([' ', '\t']).strip_prefix(';') else {
            return Ok(None);
        };
        rest = tail;
    }
}

fn token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn missing(parameter: Option<Parameter>) -> Box<Response> {
    error(
        parameter,
        "",
        "A non-empty request body is required.".into(),
    )
}

fn error(parameter: Option<Parameter>, path: &str, message: String) -> Box<Response> {
    let mut errors = vec![(path.to_string(), format!("{path} : {message}"))];
    if let Some(Parameter(parameter)) = parameter {
        errors.push((
            parameter.to_string(),
            format!("{parameter} : The {parameter} field is required."),
        ));
    }
    Box::new(crate::request_binding::ModelErrors(errors).into_response())
}
