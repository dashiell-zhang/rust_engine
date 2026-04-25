use async_trait::async_trait;
use common::app_error::AppError;
use std::time::Duration;

/// 应用层文件存储接口
#[async_trait]
pub trait AppFileStorage: Send + Sync {
    /// 上传文件
    async fn file_upload(
        &self,
        local_path: &str,
        remote_path: &str,
        is_public_read: bool,
        file_name: Option<&str>,
    ) -> Result<bool, AppError>;

    /// 下载文件
    async fn file_download(&self, remote_path: &str, local_path: &str) -> Result<bool, AppError>;

    /// 删除文件
    async fn file_delete(&self, remote_path: &str) -> Result<bool, AppError>;

    /// 获取文件临时访问地址
    fn get_file_url(&self, remote_path: &str, expiry: Duration, is_inline: bool) -> Option<String>;
}
