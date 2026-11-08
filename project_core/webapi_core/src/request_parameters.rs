use crate::body_limit::DEFAULT_BUFFERED_BODY_LIMIT_BYTES;
use axum::{
    body::{Body, Bytes, to_bytes},
    extract::Request,
    http::{
        StatusCode,
        header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE},
    },
    response::Response,
};
use futures_util::stream;
use multer::Multipart;
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::convert::Infallible;
use url::form_urlencoded;

/// 保留参数首次出现顺序，不能通过 BTreeMap 重排 Query。
pub(crate) struct Parameters(Vec<(String, String)>);
impl Serialize for Parameters {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

pub(crate) async fn read(
    request: Request,
) -> Result<(Request, Result<Parameters, String>), Box<Response>> {
    let (parts, body) = request.into_parts();
    let mut parameters = Parameters(collect_joined_pairs(form_urlencoded::parse(
        parts.uri.query().unwrap_or_default().as_bytes(),
    )));
    let length = parts
        .headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if !length.is_some_and(|length| length > 0) {
        return Ok((Request::from_parts(parts, body), Ok(parameters)));
    }
    if length.is_some_and(|length| length > DEFAULT_BUFFERED_BODY_LIMIT_BYTES as u64) {
        return Ok((
            Request::from_parts(parts, body),
            Err("请求参数超过缓冲上限".into()),
        ));
    }
    let bytes = to_bytes(body, DEFAULT_BUFFERED_BODY_LIMIT_BYTES)
        .await
        .map_err(|_| {
            Box::new(crate::error::json_error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "请求体超过允许大小",
            ))
        })?;
    let result = append_body_parameters(
        &mut parameters.0,
        parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default(),
        parts
            .headers
            .get(CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok()),
        &bytes,
    )
    .await;
    Ok((
        Request::from_parts(parts, Body::from(bytes)),
        result.map(|_| parameters),
    ))
}

/// 保留首次字段名，按 OrdinalIgnoreCase 合并同名字段并跳过空值分隔符。
pub(crate) fn append_joined(fields: &mut Vec<(String, String)>, name: String, value: String) {
    if let Some((_, existing)) = fields
        .iter_mut()
        .find(|(key, _)| common::string_helper::eq_ordinal_ignore_case(key, &name))
    {
        common::string_helper::append_string_value(existing, &value);
    } else {
        fields.push((name, value));
    }
}

fn collect_joined_pairs<'a>(
    pairs: impl IntoIterator<Item = (std::borrow::Cow<'a, str>, std::borrow::Cow<'a, str>)>,
) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    for (name, value) in pairs {
        append_joined(&mut fields, name.into_owned(), value.into_owned());
    }
    fields
}

async fn append_body_parameters(
    parameters: &mut Vec<(String, String)>,
    content_type: &str,
    encoding: Option<&str>,
    body: &Bytes,
) -> Result<(), String> {
    let media_type = content_type.split(';').next().unwrap_or_default().trim();
    let mut fields = if media_type.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
        collect_joined_pairs(form_urlencoded::parse(body))
    } else if media_type.eq_ignore_ascii_case("multipart/form-data") {
        let boundary = multer::parse_boundary(content_type).map_err(|error| error.to_string())?;
        let body_stream = stream::once(async move { Ok::<Bytes, Infallible>(body.clone()) });
        let mut multipart = Multipart::new(body_stream, boundary);
        let mut fields = Vec::new();
        let mut files = Vec::new();
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|error| error.to_string())?
        {
            let Some(disposition) = crate::form_disposition::read(field.headers())? else {
                continue;
            };
            let name = disposition.name.clone().unwrap_or_default();
            if crate::form_disposition::file_name(&disposition).is_some() {
                files.push(name);
            } else {
                append_joined(
                    &mut fields,
                    name,
                    field.text().await.map_err(|error| error.to_string())?,
                );
            }
        }
        fields.retain(|(name, _)| !files.contains(name));
        fields
    } else {
        let text = crate::request_body::read_text(encoding, body)?;
        if text.is_empty() {
            Vec::new()
        } else {
            vec![("body".to_string(), text)]
        }
    };
    common::string_helper::sort_current_culture_by(&mut fields, |field| &field.0)?;
    for (key, value) in fields {
        if parameters.iter().any(|(existing, _)| existing == &key) {
            return Err("Query 与 Body 包含重复参数名".into());
        }
        parameters.push((key, value));
    }
    Ok(())
}
