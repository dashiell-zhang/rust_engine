use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use reqwest::{
    Client, RequestBuilder, Response,
    header::{HeaderMap, HeaderName, HeaderValue},
    multipart::{Form, Part},
};
use tokio::{fs, io::AsyncWriteExt};

use crate::error_helper::CustomError;

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
        /// 内容类型
        content_type: Option<String>,
    },
}

/// 下载远程文件保存到本地
pub async fn download_file(
    http_client: &Client,
    url: &str,
    folder_path: impl AsRef<Path>,
    file_name: Option<&str>,
) -> Result<Option<PathBuf>, CustomError> {
    let response = http_client
        .get(url)
        .send()
        .await
        .map_err(|error| CustomError::with_source("远程文件下载请求失败", error))?;
    if !response.status().is_success() {
        return Ok(None);
    }

    let folder_path = folder_path.as_ref();
    fs::create_dir_all(folder_path)
        .await
        .map_err(|error| CustomError::with_source("创建下载目录失败", error))?;

    let resolved_name = file_name
        .map(str::to_string)
        .or_else(|| infer_file_name(url))
        .unwrap_or_else(|| "download.bin".to_string());
    let file_path = folder_path.join(resolved_name);
    let bytes = response
        .bytes()
        .await
        .map_err(|error| CustomError::with_source("读取远程文件内容失败", error))?;

    let mut file = fs::File::create(&file_path)
        .await
        .map_err(|error| CustomError::with_source("创建本地文件失败", error))?;
    file.write_all(&bytes)
        .await
        .map_err(|error| CustomError::with_source("写入本地文件失败", error))?;

    Ok(Some(file_path))
}

/// 以 Get 方式获取远程资源
pub async fn get(
    http_client: &Client,
    url: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, reqwest::Error> {
    apply_headers(http_client.get(url), headers).send().await
}

/// 以 Post 方式提交 Json 或 Xml 字符串
pub async fn post_string(
    http_client: &Client,
    url: &str,
    data: String,
    content_type: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, reqwest::Error> {
    apply_headers(
        http_client
            .post(url)
            .header("content-type", content_type)
            .body(data),
        headers,
    )
    .send()
    .await
}

/// 以 Delete 方式发出请求
pub async fn delete(
    http_client: &Client,
    url: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, reqwest::Error> {
    apply_headers(http_client.delete(url), headers).send().await
}

/// 以 Post 方式提交表单数据
pub async fn post_form(
    http_client: &Client,
    url: &str,
    form_items: &HashMap<String, String>,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, reqwest::Error> {
    apply_headers(http_client.post(url).form(form_items), headers)
        .send()
        .await
}

/// 以 Post 方式提交文件和表单数据
pub async fn post_form_data(
    http_client: &Client,
    url: &str,
    form_items: Vec<PostFormDataItem>,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, reqwest::Error> {
    let mut form = Form::new();

    for item in form_items {
        match item {
            PostFormDataItem::Text { key, value } => {
                form = form.text(key, value);
            }
            PostFormDataItem::File {
                key,
                file_name,
                bytes,
                content_type,
            } => {
                let part = build_file_part(file_name, bytes, content_type);
                form = form.part(key, part);
            }
        }
    }

    apply_headers(http_client.post(url).multipart(form), headers)
        .send()
        .await
}

/// 构建文件上传 Part
fn build_file_part(file_name: String, bytes: Vec<u8>, content_type: Option<String>) -> Part {
    let build_plain = || Part::bytes(bytes.clone()).file_name(file_name.clone());

    match content_type {
        Some(content_type) => match build_plain().mime_str(&content_type) {
            Ok(part) => part,
            Err(_) => build_plain(),
        },
        None => build_plain(),
    }
}

/// 应用自定义请求头
fn apply_headers(
    request: RequestBuilder,
    headers: Option<&HashMap<String, String>>,
) -> RequestBuilder {
    let Some(headers) = headers else {
        return request;
    };

    let mut header_map = HeaderMap::new();
    for (key, value) in headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            header_map.insert(name, value);
        }
    }

    request.headers(header_map)
}

/// 从 URL 推断文件名
fn infer_file_name(url: &str) -> Option<String> {
    let path = reqwest::Url::parse(url).ok()?.path().to_string();
    let name = path.rsplit('/').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}
