use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use quick_xml::{Reader, events::Event};
use reqwest::{StatusCode, header::HeaderMap};

const INVALID_DATE: &str = "Invalid signing date in Authorization header.";

/// 仅保留协议判断所需字段，不把服务端正文、对象路径或凭据写入异常。
pub(super) struct ServiceError {
    fields: BTreeMap<String, String>,
}

impl ServiceError {
    pub(super) fn parse(body: &[u8], headers: &HeaderMap) -> Result<Self, ()> {
        let decoded;
        let body = if body.is_empty() {
            if let Some(value) = headers.get("x-oss-err") {
                // Convert.FromBase64String 接受编码中的空白，非法编码直接失败。
                let encoded: Vec<_> = value
                    .as_bytes()
                    .iter()
                    .copied()
                    .filter(|byte| !byte.is_ascii_whitespace())
                    .collect();
                decoded = STANDARD.decode(encoded).map_err(|_| ())?;
                &decoded
            } else {
                body
            }
        } else {
            body
        };
        Ok(Self {
            fields: parse_fields(&String::from_utf8_lossy(body)).unwrap_or_default(),
        })
    }

    pub(super) fn retryable(&self, status: StatusCode) -> bool {
        matches!(status.as_u16(), 401 | 408 | 429)
            || status.as_u16() >= 500
            || matches!(self.field("Code"), "RequestTimeTooSkewed" | "BadRequest")
            || self.invalid_signing_date(status)
    }

    pub(super) fn clock_offset(&self, status: StatusCode, headers: &HeaderMap) -> Option<i64> {
        if !(status == StatusCode::FORBIDDEN && self.field("Code") == "RequestTimeTooSkewed"
            || self.invalid_signing_date(status))
        {
            return None;
        }
        let server_time = headers
            .get("date")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| {
                NaiveDateTime::parse_from_str(value, "%a, %d %b %Y %H:%M:%S GMT")
                    .ok()
                    .map(|value| value.and_utc())
                    .filter(|value| (1..=9999).contains(&value.year()))
            })
            .or_else(|| {
                DateTime::parse_from_rfc3339(self.field("ServerTime"))
                    .ok()
                    .map(|value| value.with_timezone(&Utc))
                    .filter(|value| (1..=9999).contains(&value.year()))
            })?;
        let offset = (server_time - Utc::now()).num_seconds();
        if status == StatusCode::BAD_REQUEST && offset.unsigned_abs() < 900 {
            return None;
        }
        Some(offset)
    }

    fn field(&self, name: &str) -> &str {
        self.fields.get(name).map(String::as_str).unwrap_or("")
    }

    fn invalid_signing_date(&self, status: StatusCode) -> bool {
        status == StatusCode::BAD_REQUEST
            && self.field("Code") == "InvalidArgument"
            && self.field("Message") == INVALID_DATE
    }
}

/// 只读取 Error 根节点的直接子字段，保留 InnerText 和重复字段最后值。
fn parse_fields(xml: &str) -> Option<BTreeMap<String, String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = true;
    let mut fields = BTreeMap::new();
    let mut depth = 0_usize;
    let mut seen_root = false;
    let mut name = String::new();
    let mut value = String::new();
    loop {
        match reader.read_event().ok()? {
            Event::Start(element) => {
                if depth == 0 {
                    if seen_root || element.name().as_ref() != "Error" {
                        return None;
                    }
                    if element
                        .try_get_attribute("xmlns")
                        .ok()?
                        .is_some_and(|attribute| !attribute.value.is_empty())
                    {
                        return None;
                    }
                    seen_root = true;
                } else if depth == 1 {
                    name = element.name().as_ref().to_owned();
                    value.clear();
                }
                depth += 1;
            }
            Event::End(_) => {
                if depth == 2 {
                    fields.insert(std::mem::take(&mut name), std::mem::take(&mut value));
                }
                depth = depth.checked_sub(1)?;
            }
            Event::Text(text) => {
                if depth >= 2 {
                    value.push_str(&text.xml10_content());
                } else if depth == 0 && !text.xml10_content().trim().is_empty() {
                    return None;
                }
            }
            Event::CData(text) => {
                if depth >= 2 {
                    value.push_str(&text.xml10_content());
                } else if depth == 0 {
                    return None;
                }
            }
            Event::GeneralRef(reference) => {
                if depth == 0 {
                    return None;
                }
                let character = match reference.resolve_char_ref().ok()? {
                    Some(character) => character,
                    None => match reference.as_ref() {
                        "amp" => '&',
                        "lt" => '<',
                        "gt" => '>',
                        "quot" => '"',
                        "apos" => '\'',
                        _ => return None,
                    },
                };
                if depth >= 2 {
                    value.push(character);
                }
            }
            Event::Eof => return (seen_root && depth == 0).then_some(fields),
            _ => {}
        }
    }
}
