use super::{READ_WRITE_TIMEOUT, integrity::TransferChecksum, request::ObjectResponse};
use common::app_error::AppError;
use reqwest::StatusCode;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
    time::{Instant, timeout_at},
};

/// 每次从头覆盖目标文件；失败保留现场，由外层重新获取对象并重置摘要。
pub(super) async fn write_response(
    response: ObjectResponse,
    local_path: &str,
) -> Result<bool, AppError> {
    let ObjectResponse {
        status,
        headers,
        mut body,
    } = response;
    let checksum = TransferChecksum::new();
    let mut deadline = Instant::now() + READ_WRITE_TIMEOUT;
    let mut file = fs::File::create(local_path)
        .await
        .map_err(|error| AppError::internal(format!("创建本地文件失败: {error}")))?;
    // 与源 SDK 的拷贝缓冲一致；单次读写共享期限，成功后才重置。
    let mut buffer = vec![0; 81_920];
    let result = async {
        loop {
            let count = timeout_at(deadline, async {
                let count = body
                    .read(&mut buffer)
                    .await
                    .map_err(|_| AppError::internal("阿里云 OSS 读取响应失败"))?;
                if count != 0 {
                    file.write_all(&buffer[..count]).await.map_err(|error| {
                        AppError::internal(format!("写入本地文件失败: {error}"))
                    })?;
                }
                Ok::<_, AppError>(count)
            })
            .await
            .map_err(|_| AppError::internal("阿里云 OSS 下载读写超时"))??;
            if count == 0 {
                break;
            }
            checksum.update(&buffer[..count]);
            deadline = Instant::now() + READ_WRITE_TIMEOUT;
        }
        checksum.verify(&headers)
    }
    .await;
    // Tokio 文件写入可能仍在后台执行。即使拷贝失败也先等待收尾，
    // 防止上一轮未完成的写入覆盖下一轮重新下载的数据。
    let flushed = file
        .flush()
        .await
        .map_err(|error| AppError::internal(format!("刷新本地文件失败: {error}")));
    result?;
    flushed?;
    Ok(status == StatusCode::OK)
}
