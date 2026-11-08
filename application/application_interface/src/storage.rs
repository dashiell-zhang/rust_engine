use async_trait::async_trait;
use chrono::TimeDelta;
use common::app_error::AppError;

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

    /// 获取临时地址，时长保留 TimeSpan 的正负值；无地址与生成失败分别返回 None 和错误。
    fn get_file_url(
        &self,
        remote_path: &str,
        expiry: TimeDelta,
        is_inline: bool,
    ) -> Result<Option<String>, AppError>;
}
