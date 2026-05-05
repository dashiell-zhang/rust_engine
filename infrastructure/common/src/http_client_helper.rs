use std::{
    collections::HashMap,
    ffi::OsString,
    io,
    path::{Component, Path, PathBuf},
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
        .map(sanitize_file_name)
        .transpose()?
        .or_else(|| infer_file_name(url))
        .unwrap_or_else(|| "download.bin".to_string());
    let file_path = folder_path.join(resolved_name);
    let temp_file_path = temporary_download_path(&file_path);

    let mut file = fs::File::create(&temp_file_path)
        .await
        .map_err(|error| CustomError::with_source("创建本地文件失败", error))?;
    let mut response = response;
    while let Some(bytes) = response
        .chunk()
        .await
        .map_err(|error| CustomError::with_source("读取远程文件内容失败", error))?
    {
        file.write_all(&bytes)
            .await
            .map_err(|error| CustomError::with_source("写入本地文件失败", error))?;
    }
    file.flush()
        .await
        .map_err(|error| CustomError::with_source("刷新本地文件失败", error))?;
    drop(file);

    if let Err(error) = replace_file(&temp_file_path, &file_path).await {
        let _ = fs::remove_file(&temp_file_path).await;
        return Err(CustomError::with_source("保存下载文件失败", error));
    }

    Ok(Some(file_path))
}

/// 以 Get 方式获取远程资源
pub async fn get(
    http_client: &Client,
    url: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
    apply_headers(http_client.get(url), headers)?
        .send()
        .await
        .map_err(|error| CustomError::with_source("远程请求失败", error))
}

/// 以 Post 方式提交 Json 或 Xml 字符串
pub async fn post_string(
    http_client: &Client,
    url: &str,
    data: String,
    content_type: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
    apply_headers(
        http_client
            .post(url)
            .header("content-type", content_type)
            .body(data),
        headers,
    )?
    .send()
    .await
    .map_err(|error| CustomError::with_source("远程请求失败", error))
}

/// 以 Delete 方式发出请求
pub async fn delete(
    http_client: &Client,
    url: &str,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
    apply_headers(http_client.delete(url), headers)?
        .send()
        .await
        .map_err(|error| CustomError::with_source("远程请求失败", error))
}

/// 以 Post 方式提交表单数据
pub async fn post_form(
    http_client: &Client,
    url: &str,
    form_items: &HashMap<String, String>,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
    apply_headers(http_client.post(url).form(form_items), headers)?
        .send()
        .await
        .map_err(|error| CustomError::with_source("远程请求失败", error))
}

/// 以 Post 方式提交文件和表单数据
pub async fn post_form_data(
    http_client: &Client,
    url: &str,
    form_items: Vec<PostFormDataItem>,
    headers: Option<&HashMap<String, String>>,
) -> Result<Response, CustomError> {
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

    apply_headers(http_client.post(url).multipart(form), headers)?
        .send()
        .await
        .map_err(|error| CustomError::with_source("远程请求失败", error))
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
) -> Result<RequestBuilder, CustomError> {
    let Some(headers) = headers else {
        return Ok(request);
    };

    let mut header_map = HeaderMap::new();
    for (key, value) in headers {
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|error| CustomError::with_source(format!("请求头名称无效: {key}"), error))?;
        let value = HeaderValue::from_str(value)
            .map_err(|error| CustomError::with_source(format!("请求头值无效: {key}"), error))?;
        header_map.insert(name, value);
    }

    Ok(request.headers(header_map))
}

/// 从 URL 推断文件名
fn infer_file_name(url: &str) -> Option<String> {
    let path = reqwest::Url::parse(url).ok()?.path().to_string();
    let name = path.rsplit('/').next()?.trim();
    if name.is_empty() {
        None
    } else {
        sanitize_file_name(name).ok()
    }
}

/// 校验并提取纯文件名，避免写出目标目录
fn sanitize_file_name(file_name: &str) -> Result<String, CustomError> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return Err(CustomError::new("下载文件名不能为空"));
    }

    let path = Path::new(trimmed);
    let mut components = path.components();
    let Some(Component::Normal(name)) = components.next() else {
        return Err(CustomError::new("下载文件名无效"));
    };

    if components.next().is_some() {
        return Err(CustomError::new("下载文件名不能包含路径"));
    }

    let name = PathBuf::from(OsString::from(name))
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CustomError::new("下载文件名无效"))?
        .to_string();

    Ok(name)
}

/// 生成下载临时文件路径
fn temporary_download_path(file_path: &Path) -> PathBuf {
    let mut temp_name = file_path
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| "download.bin".into());
    temp_name.push(format!(".{}.tmp", uuid::Uuid::now_v7()));

    file_path.with_file_name(temp_name)
}

/// 使用临时文件替换目标文件
async fn replace_file(temp_file_path: &Path, file_path: &Path) -> io::Result<()> {
    match fs::rename(temp_file_path, file_path).await {
        Ok(()) => Ok(()),
        Err(_) if file_path.exists() => replace_existing_file(temp_file_path, file_path).await,
        Err(error) => Err(error),
    }
}

/// 替换已存在的目标文件，失败时尽量恢复旧文件
async fn replace_existing_file(temp_file_path: &Path, file_path: &Path) -> io::Result<()> {
    let backup_path = temporary_replace_backup_path(file_path);
    fs::rename(file_path, &backup_path).await?;

    match fs::rename(temp_file_path, file_path).await {
        Ok(()) => {
            let _ = fs::remove_file(&backup_path).await;
            Ok(())
        }
        Err(error) => {
            if let Err(restore_error) = fs::rename(&backup_path, file_path).await {
                return Err(io::Error::new(
                    error.kind(),
                    format!("替换文件失败: {error}; 恢复原文件失败: {restore_error}"),
                ));
            }

            Err(error)
        }
    }
}

/// 生成替换备份文件路径
fn temporary_replace_backup_path(file_path: &Path) -> PathBuf {
    let mut temp_name = file_path
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_else(|| "download.bin".into());
    temp_name.push(format!(".{}.bak", uuid::Uuid::now_v7()));

    file_path.with_file_name(temp_name)
}
