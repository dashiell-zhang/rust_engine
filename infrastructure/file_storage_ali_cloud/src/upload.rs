use std::{io::SeekFrom, sync::Arc};

use common::app_error::AppError;
use futures_util::stream;
use reqwest::Body;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;

use super::{integrity::TransferChecksum, request::RequestDeadline};

/// 重试复用同一打开的文件，不重新解析可能已被替换的本地路径。
pub(super) struct UploadSource {
    file: Arc<Mutex<fs::File>>,
    pub(super) length: u64,
}

impl UploadSource {
    pub(super) async fn open(path: &str) -> Result<Self, AppError> {
        let file = fs::File::open(path)
            .await
            .map_err(|error| AppError::internal(format!("打开本地文件失败: {error}")))?;
        let length = file
            .metadata()
            .await
            .map_err(|error| AppError::internal(format!("读取本地文件信息失败: {error}")))?
            .len();
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            length,
        })
    }

    pub(super) async fn body(
        &self,
        checksum: TransferChecksum,
        deadline: RequestDeadline,
        cancellation: CancellationToken,
    ) -> Result<Body, AppError> {
        self.file
            .lock()
            .await
            .seek(SeekFrom::Start(0))
            .await
            .map_err(|error| AppError::internal(format!("重置上传文件失败: {error}")))?;
        let reset_deadline = self.length >= 256 * 1024;
        let state = (self.file.clone(), checksum, deadline, cancellation);
        let stream = stream::try_unfold(state, move |state| async move {
            let (file, checksum, deadline, cancellation) = &state;
            let mut buffer = vec![0; 81_920];
            let count = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Ok::<_, std::io::Error>(None),
                count = async { file.lock().await.read(&mut buffer).await } => count?,
            };
            if count == 0 || cancellation.is_cancelled() {
                return Ok(None);
            }
            buffer.truncate(count);
            checksum.update(&buffer);
            if reset_deadline {
                deadline.progress();
            }
            Ok(Some((buffer, state)))
        });
        Ok(Body::wrap_stream(stream))
    }
}
