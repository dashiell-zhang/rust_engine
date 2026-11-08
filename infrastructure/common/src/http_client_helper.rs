use std::collections::HashMap;

use crate::error_helper::CustomError;
use reqwest::{
    Body, Client, RequestBuilder, Response, ResponseBuilderExt,
    header::{HeaderName, HeaderValue},
    multipart::{Form, Part},
};

mod content_disposition;
pub use content_disposition::{ContentDisposition, parse_content_disposition};
mod download;
mod options;
pub use download::download_file;
pub use options::RequestOptions;

/// 表单数据项
pub enum PostFormDataItem {
    /// 文本字段
    Text {
        /// 表单键
        key: String,
        /// 表单值
        value: String,
    },

    /// 文件字段
    File {
        /// 表单键
        key: String,
        /// 文件名
        file_name: String,
        /// 文件内容
        bytes: Vec<u8>,
    },

    /// 文件流由请求持有，发送结束或取消时释放，不预先读入内存。
    FileStream {
        key: String,
        file_name: String,
        body: Body,
        /// 已知的剩余长度；未知时采用流式传输。
        length: Option<u64>,
    },
    /// 可重新定位的文件流，可供签名处理器读取摘要后复位。
    FileHandle {
        key: String,
        file_name: String,
        file: tokio::fs::File,
        length: Option<u64>,
    },
}

/// 以 Get 方式获取远程资源
pub async fn get(
    http_client: &Client,
    url: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
    send_buffered(apply_headers(http_client.get(url), headers)?).await
}

/// 以 Post 方式提交 Json 或 Xml 字符串
pub async fn post_string(
    http_client: &Client,
    url: &str,
    data: String,
    data_type: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
    send_buffered(build_post_string(
        http_client,
        url,
        data,
        data_type,
        headers,
    )?)
    .await
}

/// 组装字符串请求，供宿主的出站处理器发送。
pub fn build_post_string(
    http_client: &Client,
    url: &str,
    data: String,
    data_type: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<RequestBuilder, CustomError> {
    let content_type = if data_type.eq_ignore_ascii_case("json") {
        "application/json; charset=utf-8"
    } else if data_type.eq_ignore_ascii_case("xml") {
        "text/xml; charset=utf-8"
    } else {
        return Err(CustomError::new("type 无效，只能是 json 或 xml"));
    };
    apply_headers(
        http_client
            .post(url)
            .header("content-type", content_type)
            .body(data),
        headers,
    )
}

/// 以 Delete 方式发出请求
pub async fn delete(
    http_client: &Client,
    url: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
    send_buffered(apply_headers(http_client.delete(url), headers)?).await
}

/// 以 Post 方式提交表单数据
pub async fn post_form(
    http_client: &Client,
    url: &str,
    form_items: &HashMap<String, String>,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
    send_buffered(build_post_form(http_client, url, form_items, headers)?).await
}

pub fn build_post_form(
    http_client: &Client,
    url: &str,
    form_items: &HashMap<String, String>,
    headers: Option<&HashMap<String, String>>,
) -> Result<RequestBuilder, CustomError> {
    // FormUrlEncodedContent 沿用 URI 非保留字符规则，和浏览器表单对 * / ~ 的处理不同。
    let mut body = String::new();
    for (key, value) in form_items {
        if !body.is_empty() {
            body.push('&');
        }
        append_form_value(&mut body, key);
        body.push('=');
        append_form_value(&mut body, value);
    }
    apply_headers(
        http_client.post(url).body(body).header(
            "content-type",
            "application/x-www-form-urlencoded; charset=utf-8",
        ),
        headers,
    )
}

fn append_form_value(output: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(char::from(byte));
            }
            b' ' => output.push('+'),
            _ => {
                output.push('%');
                output.push(char::from(HEX[usize::from(byte >> 4)]));
                output.push(char::from(HEX[usize::from(byte & 15)]));
            }
        }
    }
}

/// 以 Post 方式提交文件和表单数据
pub async fn post_form_data(
    http_client: &Client,
    url: &str,
    form_items: Vec<PostFormDataItem>,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
    send_buffered(build_post_form_data(http_client, url, form_items, headers)?).await
}

pub fn build_post_form_data(
    http_client: &Client,
    url: &str,
    form_items: Vec<PostFormDataItem>,
    headers: Option<&HashMap<String, String>>,
) -> Result<RequestBuilder, CustomError> {
    for item in &form_items {
        validate_form_item(item)?;
    }
    let mut form = Form::new();

    for item in form_items {
        match item {
            PostFormDataItem::Text { key, value } => {
                let part = Part::text(value)
                    .mime_str("text/plain; charset=utf-8")
                    .map_err(|error| CustomError::with_source("文本表单类型无效", error))?;
                form = form.part(key, part);
            }
            PostFormDataItem::File {
                key,
                file_name,
                bytes,
            } => {
                let part = Part::bytes(bytes).file_name(file_name);
                form = form.part(key, part);
            }
            PostFormDataItem::FileStream {
                key,
                file_name,
                body,
                length,
            } => {
                let part = match length {
                    Some(length) => Part::stream_with_length(body, length),
                    None => Part::stream(body),
                }
                .file_name(file_name);
                form = form.part(key, part);
            }
            PostFormDataItem::FileHandle {
                key,
                file_name,
                file,
                length,
            } => {
                let part = match length {
                    Some(length) => Part::stream_with_length(file, length),
                    None => Part::stream(file),
                }
                .file_name(file_name);
                form = form.part(key, part);
            }
        }
    }

    apply_headers(http_client.post(url).multipart(form), headers)
}

pub fn validate_form_item(item: &PostFormDataItem) -> Result<(), CustomError> {
    let (key, file_name) = match item {
        PostFormDataItem::Text { key, .. } => (key, None),
        PostFormDataItem::File { key, file_name, .. }
        | PostFormDataItem::FileStream { key, file_name, .. }
        | PostFormDataItem::FileHandle { key, file_name, .. } => (key, Some(file_name)),
    };
    if key.trim().is_empty() {
        return Err(CustomError::new("表单项 Key 不能为空"));
    }
    if file_name.is_some_and(|name| name.trim().is_empty()) {
        return Err(CustomError::new("上传文件时 FileName 属性值不能为空"));
    }
    Ok(())
}

/// 普通扩展方法在正文完整接收后返回，HTTP 非成功状态仍交给调用方处理。
/// 下载工具采用独立的流式路径；取消此 Future 同时释放请求和响应体。
pub async fn send_buffered(request: RequestBuilder) -> Result<Response, CustomError> {
    let response = request
        .send()
        .await
        .map_err(|error| CustomError::with_source("远程请求失败", error))?;
    buffer_response(response).await
}

/// 保留响应元数据，供宿主出站处理器在读取响应头之后完成缓冲。
pub async fn buffer_response(mut response: Response) -> Result<Response, CustomError> {
    const MAX_BUFFER_SIZE: usize = i32::MAX as usize;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BUFFER_SIZE as u64)
    {
        return Err(CustomError::new("远程响应超过最大缓冲长度"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| CustomError::with_source("读取远程响应失败", error))?
    {
        if chunk.len() > MAX_BUFFER_SIZE - body.len() {
            return Err(CustomError::new("远程响应超过最大缓冲长度"));
        }
        body.extend_from_slice(&chunk);
    }
    let mut buffered = http::Response::builder()
        .status(response.status())
        .version(response.version())
        .url(response.url().clone())
        .body(body)
        .map_err(|error| CustomError::with_source("构造远程响应失败", error))?;
    *buffered.headers_mut() = std::mem::take(response.headers_mut());
    buffered
        .extensions_mut()
        .extend(std::mem::take(response.extensions_mut()));
    Ok(buffered.into())
}

/// 应用自定义请求头
pub fn apply_headers(
    request: RequestBuilder,
    headers: Option<&HashMap<String, String>>,
) -> Result<RequestBuilder, CustomError> {
    let Some(headers) = headers else {
        return Ok(request);
    };

    let (client, request) = request.build_split();
    let mut request =
        request.map_err(|error| CustomError::with_source("构造远程请求失败", error))?;
    for (key, value) in headers {
        // TryAddWithoutValidation 对无效名称返回 false；没有内容时忽略内容头。
        let Ok(name) = HeaderName::from_bytes(key.as_bytes()) else {
            continue;
        };
        let content_header = is_content_header(&name);
        if content_header && request.body().is_none() {
            continue;
        }
        let value = HeaderValue::from_str(value)
            .map_err(|error| CustomError::with_source(format!("请求头值无效: {key}"), error))?;
        if content_header {
            request.headers_mut().insert(name, value);
        } else {
            request.headers_mut().append(name, value);
        }
    }

    Ok(RequestBuilder::from_parts(client, request))
}

/// 标准内容头与普通请求头分别采用替换和追加规则。
pub fn is_content_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "allow"
            | "content-disposition"
            | "content-encoding"
            | "content-language"
            | "content-length"
            | "content-location"
            | "content-md5"
            | "content-range"
            | "content-type"
            | "expires"
            | "last-modified"
    )
}
