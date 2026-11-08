//! 对齐 Kestrel 的路径解码和 PathString 的 URI 表示；查询串不参与路径转换。

use std::fmt::Write;

use axum::http::{Extensions, Uri};

#[derive(Clone)]
pub(crate) struct RequestPath(String);

impl RequestPath {
    pub(crate) fn parse(path: &str) -> Result<Self, ()> {
        let input = path.as_bytes();
        let mut decoded = Vec::with_capacity(input.len());
        let mut index = 0;
        while index < input.len() {
            let Some(first) = unescape(input, index) else {
                decoded.push(input[index]);
                index += 1;
                continue;
            };
            if first == 0 {
                return Err(());
            }
            if first == b'/' {
                // 编码斜杠不成为段分隔符，保留原有十六进制大小写。
                decoded.extend_from_slice(&input[index..index + 3]);
                index += 3;
                continue;
            }
            let width = match first {
                0..=0x7f => 1,
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => 0,
            };
            let mut scalar = [0; 4];
            scalar[0] = first;
            let mut valid = width > 0;
            for (offset, byte) in scalar.iter_mut().enumerate().take(width).skip(1) {
                match unescape(input, index + offset * 3) {
                    Some(next) if next & 0xc0 == 0x80 => *byte = next,
                    _ => valid = false,
                }
            }
            if valid && std::str::from_utf8(&scalar[..width]).is_ok() {
                decoded.extend_from_slice(&scalar[..width]);
                index += width * 3;
            } else {
                // 非法/截断 UTF-8 保留编码，不用替换字符，也不二次解码。
                decoded.extend_from_slice(&input[index..index + 3]);
                index += 3;
            }
        }
        let decoded = String::from_utf8(decoded).map_err(|_| ())?;
        Ok(Self(remove_dot_segments(&decoded)))
    }

    pub(crate) fn value(&self) -> &str {
        &self.0
    }

    pub(crate) fn path_and_query(&self, uri: &Uri) -> String {
        let mut result = escape(&self.0, true);
        if let Some(query) = uri.query() {
            result.push('?');
            result.push_str(query);
        }
        result
    }
}

pub(crate) fn value<'a>(uri: &'a Uri, extensions: &'a Extensions) -> &'a str {
    extensions
        .get::<RequestPath>()
        .map_or_else(|| uri.path(), RequestPath::value)
}

pub(crate) fn path_and_query(uri: &Uri, extensions: &Extensions) -> String {
    extensions.get::<RequestPath>().map_or_else(
        || {
            uri.path_and_query()
                .map_or("/", |value| value.as_str())
                .to_owned()
        },
        |path| path.path_and_query(uri),
    )
}

/// Axum 会再解码一次捕获参数，因此此表示必须转义所有百分号。
pub(crate) fn routing_path(path: &str) -> String {
    escape(path, false)
}

fn unescape(input: &[u8], index: usize) -> Option<u8> {
    if input.get(index) != Some(&b'%') {
        return None;
    }
    Some(hex(*input.get(index + 1)?)? * 16 + hex(*input.get(index + 2)?)?)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn escape(path: &str, preserve_escape: bool) -> String {
    let input = path.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        let byte = input[index];
        if preserve_escape && unescape(input, index).is_some() {
            output.push_str(&path[index..index + 3]);
            index += 3;
        } else {
            if byte.is_ascii_alphanumeric() || b"!$&'()*+,-./:;=@_~".contains(&byte) {
                output.push(char::from(byte));
            } else {
                write!(output, "%{byte:02X}").expect("写入路径字符串");
            }
            index += 1;
        }
    }
    output
}

fn remove_dot_segments(path: &str) -> String {
    if !path.starts_with('/') || !path.contains("/.") {
        return path.to_owned();
    }
    let mut output = Vec::new();
    let mut segments = path[1..].split('/').peekable();
    while let Some(segment) = segments.next() {
        match segment {
            "." | ".." => {
                if segment == ".." {
                    output.pop();
                }
                if segments.peek().is_none() {
                    output.push("");
                }
            }
            _ => output.push(segment),
        }
    }
    format!("/{}", output.join("/"))
}
