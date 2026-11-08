use std::path::{Path, PathBuf};

use crate::AppFileStorage;
use async_trait::async_trait;
use chrono::TimeDelta;
use common::app_error::AppError;
use tokio::fs;

/// 本地文件存储
pub struct LocalFileStorage {
    /// 文件根目录
    root_path: PathBuf,
}

impl LocalFileStorage {
    /// 创建本地文件存储
    pub fn new(root_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: root_path.into(),
        }
    }

    /// 规范化相对路径
    fn normalize_relative_path(&self, relative_path: &str) -> Result<PathBuf, AppError> {
        let normalized_path = relative_path.replace('\\', "/");
        let relative = Path::new(&normalized_path);

        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(AppError::bad_request("文件路径非法"));
        }

        Ok(self.root_path.join(relative))
    }
}

#[async_trait]
impl AppFileStorage for LocalFileStorage {
    /// 上传本地文件
    async fn file_upload(
        &self,
        local_path: &str,
        remote_path: &str,
        _is_public_read: bool,
        _file_name: Option<&str>,
    ) -> Result<bool, AppError> {
        let source = Path::new(local_path);
        let file_name = source
            .file_name()
            .and_then(|item| item.to_str())
            .ok_or_else(|| AppError::bad_request("本地文件路径非法"))?;
        let target = self.normalize_relative_path(&format!("{remote_path}/{file_name}"))?;

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| AppError::internal(format!("创建文件目录失败: {error}")))?;
        }

        fs::copy(source, target)
            .await
            .map_err(|error| AppError::internal(format!("复制文件失败: {error}")))?;

        Ok(true)
    }

    /// 下载文件到本地
    async fn file_download(&self, remote_path: &str, local_path: &str) -> Result<bool, AppError> {
        let source = self.normalize_relative_path(remote_path)?;
        let target = Path::new(local_path);

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| AppError::internal(format!("创建文件目录失败: {error}")))?;
        }

        match fs::copy(source, target).await {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(AppError::internal(format!("下载文件失败: {error}"))),
        }
    }

    /// 删除文件
    async fn file_delete(&self, remote_path: &str) -> Result<bool, AppError> {
        let path = self.normalize_relative_path(remote_path)?;

        match fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(AppError::internal(format!("删除文件失败: {error}"))),
        }
    }

    /// 本地文件存储不生成临时签名地址
    fn get_file_url(
        &self,
        _remote_path: &str,
        _expiry: TimeDelta,
        _is_inline: bool,
    ) -> Result<Option<String>, AppError> {
        Ok(None)
    }
}
