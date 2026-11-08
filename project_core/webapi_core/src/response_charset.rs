//! MVC 文本格式化器的字符集选择；不应用到文件或直接写出的 JsonResult。
use std::{
    cmp::Ordering,
    io,
    pin::Pin,
    task::{Context, Poll, ready},
};

use axum::{
    body::{Body, Bytes},
    http::{HeaderValue, header},
    response::Response,
};
use http_body::{Frame, SizeHint};

pub(crate) fn apply(mut response: Response, accept: &[HeaderValue]) -> Response {
    if response
        .extensions()
        .get::<crate::response_format::OutputKind>()
        .is_none()
        || !select_utf16(accept)
    {
        return response;
    }
    let Some(content_type) = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return response;
    };
    // 当前文本结果的媒体类型由提供方声明，字符集转换不改变媒体类型。
    let media_type = content_type.split(';').next().unwrap_or(content_type);
    let Ok(content_type) = HeaderValue::from_str(&format!("{media_type}; charset=utf-16")) else {
        return response;
    };
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    response.headers_mut().remove(header::CONTENT_LENGTH);
    response.map(|body| {
        Body::new(Utf16Body {
            inner: Box::pin(body),
            pending: Vec::new(),
            ended: false,
        })
    })
}

fn select_utf16(headers: &[HeaderValue]) -> bool {
    let mut values = Vec::new();
    for header in headers {
        let Ok(value) = header.to_str() else { continue };
        for item in value.split(',') {
            if let Some((rest, (name, quality))) = item.char_indices().find_map(|(index, _)| {
                let rest = &item[index..];
                crate::response_compression::parse_item(rest).map(|value| (rest, value))
            }) {
                values.push((name, quality, rest.contains(';')));
            }
        }
    }
    // 对齐 TextOutputFormatter.Sort：仅存在显式的非零质量时排序并剔除零质量。
    // 无此类质量时源实现直接返回原列表，包括其中的 q=0。
    if values
        .iter()
        .any(|(_, quality, explicit)| *explicit && *quality != 0)
    {
        let mut sorted: Vec<(&str, u32, bool)> = Vec::new();
        for value in values.into_iter().filter(|(_, quality, _)| *quality != 0) {
            let mut low = 0;
            let mut high = sorted.len();
            while low < high {
                let middle = low + (high - low - 1) / 2;
                let current = sorted[middle];
                let order = current
                    .1
                    .cmp(&value.1)
                    .then_with(|| (current.0 != "*").cmp(&(value.0 != "*")));
                match order {
                    Ordering::Less => low = middle + 1,
                    Ordering::Greater => high = middle,
                    Ordering::Equal => {
                        low = middle + 1;
                        break;
                    }
                }
            }
            sorted.insert(low, value);
        }
        sorted.reverse();
        values = sorted;
    }
    for (name, _, _) in values {
        if name.eq_ignore_ascii_case("utf-16") {
            return true;
        }
        if name.eq_ignore_ascii_case("utf-8") || name == "*" {
            return false;
        }
    }
    false
}

struct Utf16Body {
    inner: Pin<Box<Body>>,
    // UTF-8 字符跨帧时最多保留三个未完成字节，不预先收集响应流。
    pending: Vec<u8>,
    ended: bool,
}

impl http_body::Body for Utf16Body {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        if self.ended {
            return Poll::Ready(None);
        }
        match ready!(self.inner.as_mut().poll_frame(cx)) {
            Some(Ok(frame)) => match frame.into_data() {
                Ok(data) => {
                    self.pending.extend_from_slice(&data);
                    let length = match std::str::from_utf8(&self.pending) {
                        Ok(value) => value.len(),
                        Err(error) if error.error_len().is_none() => error.valid_up_to(),
                        Err(_) => return self.invalid_utf8(),
                    };
                    let text = std::str::from_utf8(&self.pending[..length])
                        .expect("validated UTF-8 prefix");
                    let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
                    self.pending = self.pending[length..].to_vec();
                    Poll::Ready(Some(Ok(Frame::data(bytes.into()))))
                }
                Err(frame) if self.pending.is_empty() => Poll::Ready(Some(Ok(frame))),
                Err(_) => self.invalid_utf8(),
            },
            Some(Err(error)) => {
                self.ended = true;
                Poll::Ready(Some(Err(error)))
            }
            None if self.pending.is_empty() => {
                self.ended = true;
                Poll::Ready(None)
            }
            None => self.invalid_utf8(),
        }
    }

    fn is_end_stream(&self) -> bool {
        self.ended || (self.pending.is_empty() && self.inner.is_end_stream())
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }
}

impl Utf16Body {
    fn invalid_utf8(&mut self) -> Poll<Option<Result<Frame<Bytes>, axum::Error>>> {
        self.ended = true;
        self.pending.clear();
        Poll::Ready(Some(Err(axum::Error::new(io::Error::new(
            io::ErrorKind::InvalidData,
            "响应文本不是完整的 UTF-8",
        )))))
    }
}
