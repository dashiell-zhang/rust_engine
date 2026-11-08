//! 流式下载与文件名选择，供普通远程上传和其他调用方共用。
use super::parse_content_disposition;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use reqwest::{Client, Response, header};
use tokio::{fs, io::AsyncWriteExt};

use crate::{error_helper::CustomError, io_helper::TemporaryFileGuard};

/// 下载成功响应到同目录临时文件，完整接收后再替换目标文件。
/// 客户端应关闭整包超时；header_timeout 只约束响应头，正文跟随调用 future 取消。
pub async fn download_file(
    http_client: &Client,
    url: &str,
    folder_path: impl AsRef<Path>,
    file_name: Option<&str>,
    header_timeout: Duration,
) -> Result<PathBuf, CustomError> {
    let folder = folder_path.as_ref();
    fs::create_dir_all(folder)
        .await
        .map_err(|error| CustomError::with_source("创建下载目录失败", error))?;
    let mut response = tokio::time::timeout(header_timeout, http_client.get(url).send())
        .await
        .map_err(|error| CustomError::with_source("远程文件响应头超时", error))?
        .map_err(|error| CustomError::with_source("远程文件下载请求失败", error))?;
    if !response.status().is_success() {
        return Err(CustomError::new(format!(
            "远程文件返回非成功状态码: {}",
            response.status()
        )));
    }
    let target = folder.join(download_file_name(url, &response, file_name));
    let mut temporary_name = target.as_os_str().to_os_string();
    temporary_name.push(format!(".{}.download", uuid::Uuid::new_v4().simple()));
    let temporary = PathBuf::from(temporary_name);
    // 等待文件创建时取消，阻塞任务的结果也拥有清理守卫。
    let (mut guard, output) = tokio::task::spawn_blocking(move || {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        Ok::<_, std::io::Error>((TemporaryFileGuard::new(temporary), file))
    })
    .await
    .map_err(|error| CustomError::with_source("创建下载文件任务失败", error))?
    .map_err(|error| CustomError::with_source("创建下载文件失败", error))?;
    let mut output = fs::File::from_std(output);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| CustomError::with_source("读取远程文件内容失败", error))?
    {
        output
            .write_all(&chunk)
            .await
            .map_err(|error| CustomError::with_source("写入下载文件失败", error))?;
    }
    output
        .flush()
        .await
        .map_err(|error| CustomError::with_source("刷新下载文件失败", error))?;
    drop(output);
    // 同目录发布为一个同步原子步骤，避免取消后异步 rename 才生成无人持有的目标。
    std::fs::rename(guard.path(), &target)
        .map_err(|error| CustomError::with_source("保存下载文件失败", error))?;
    guard.preserve();
    Ok(target)
}

/// 显式名称、filename*、filename、初始 URL 的优先顺序与源工具一致。
fn download_file_name(url: &str, response: &Response, explicit: Option<&str>) -> String {
    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|header| header.to_str().ok())
        .and_then(parse_content_disposition);
    let selected = explicit
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            disposition
                .as_ref()
                .and_then(|names| names.filename_star.clone())
                .filter(|name| !name.trim().is_empty())
        })
        .or_else(|| {
            disposition
                .and_then(|names| names.filename)
                .filter(|name| !name.trim().is_empty())
        })
        .or_else(|| {
            let url = reqwest::Url::parse(url).ok()?;
            let local = urlencoding::decode(url.path()).ok()?;
            let name = local.rsplit(std::path::is_separator).next()?;
            let name = name.replace('+', " ");
            Some(String::from_utf8_lossy(&urlencoding::decode_binary(name.as_bytes())).into_owned())
        })
        .unwrap_or_default();
    let name = selected
        .trim_matches('"')
        .rsplit(std::path::is_separator)
        .next()
        .unwrap_or_default();
    if name.trim().is_empty() {
        uuid::Uuid::new_v4().simple().to_string()
    } else {
        name.into()
    }
}
