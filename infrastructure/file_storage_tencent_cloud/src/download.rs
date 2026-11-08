use std::{path::Path, time::Duration};

use common::app_error::AppError;
use futures_util::{StreamExt, stream::FuturesUnordered};
use reqwest::{
    Method, Response, StatusCode,
    header::{AUTHORIZATION, CONTENT_LENGTH, HOST, RANGE},
};
use tokio::{fs, io::AsyncWriteExt, time::timeout};
use tokio_util::sync::CancellationToken;

use super::TencentCosFileStorage;

const DIVISION_SIZE: i64 = 20 * 1024 * 1024;
const SLICE_SIZE: i64 = 10 * 1024 * 1024;
const CONCURRENT_PARTS: usize = 5;
const TRANSFER_ROUNDS: usize = 3;
const REQUEST_ATTEMPTS: usize = 4;
// 源任务在 100ms 轮询中检查等待时间严格大于 30000ms。
const COMPLETION_TIMEOUT: Duration = Duration::from_millis(30_100);

struct DownloadPart {
    path: String,
    range: String,
}

struct DownloadFailure {
    error: AppError,
    terminal: bool,
}

impl DownloadFailure {
    fn client(message: impl Into<String>) -> Self {
        Self {
            error: AppError::internal(message.into()),
            terminal: false,
        }
    }

    fn cancelled() -> Self {
        Self::client("腾讯云 COS 下载已取消")
    }
}

impl TencentCosFileStorage {
    /// 默认 TransferManager：先 HEAD，按大小分片，全部成功后合并并检查总长度。
    pub(super) async fn download_object(
        &self,
        object_key: &str,
        local_path: &str,
        directory: &str,
    ) -> Result<bool, AppError> {
        let cancellation = CancellationToken::new();
        let _cancel_on_drop = cancellation.clone().drop_guard();
        let response = self
            .download_response(Method::HEAD, object_key, None, &cancellation)
            .await
            .map_err(|failure| failure.error)?;
        // HeadObjectResult 使用 Int64.TryParse，缺失或非法长度保留为零。
        let length = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(0);
        drop(response);
        let file_name = local_path.rsplit('/').next().unwrap_or_default();
        let prefix = format!(".{file_name}.cosresumable.");
        let count = if length < DIVISION_SIZE {
            1
        } else {
            (length - 1) / SLICE_SIZE + 1
        };
        let parts: Vec<_> = (0..count)
            .map(|index| {
                let start = if count == 1 { 0 } else { index * SLICE_SIZE };
                let end = if count == 1 {
                    length - 1
                } else {
                    (start + SLICE_SIZE - 1).min(length - 1)
                };
                DownloadPart {
                    path: format!("{directory}{prefix}{}", index + 1),
                    // 零长度对象仍按 SDK 发送 bytes=0-，不跳过远程 GET。
                    range: if end < start {
                        format!("bytes={start}-")
                    } else {
                        format!("bytes={start}-{end}")
                    },
                }
            })
            .collect();
        let mut complete = vec![false; parts.len()];
        let mut last_status = StatusCode::OK;
        let mut last_error = None;
        for _ in 0..TRANSFER_ROUNDS {
            let round = cancellation.child_token();
            let mut pending = complete
                .iter()
                .enumerate()
                .filter_map(|(index, done)| (!done).then_some(index))
                .collect::<Vec<_>>()
                .into_iter();
            let mut active = FuturesUnordered::new();
            for index in pending.by_ref().take(CONCURRENT_PARTS) {
                active.push(self.download_part(object_key, index, &parts[index], &round));
            }
            let mut terminal = false;
            while !active.is_empty() {
                // 源任务只有在本轮所有分片已发起后才观察完成数量是否长时间不变。
                let result = if pending.len() == 0 && !round.is_cancelled() {
                    match timeout(COMPLETION_TIMEOUT, active.next()).await {
                        Ok(result) => result,
                        Err(_) => {
                            round.cancel();
                            last_error = Some(AppError::internal("腾讯云 COS 分片等待完成超时"));
                            continue;
                        }
                    }
                } else {
                    active.next().await
                };
                let Some((index, result)) = result else { break };
                match result {
                    Ok(status) => {
                        complete[index] = true;
                        last_status = status;
                    }
                    Err(failure) => {
                        if failure.terminal {
                            terminal = true;
                            round.cancel();
                            last_error = Some(failure.error);
                        } else if !round.is_cancelled() {
                            last_error = Some(failure.error);
                        }
                    }
                }
                if !round.is_cancelled()
                    && let Some(index) = pending.next()
                {
                    active.push(self.download_part(object_key, index, &parts[index], &round));
                }
            }
            // 取消后已等待各分片刷新文件，不能让旧写入落到下一轮覆盖之后。
            if terminal {
                return Err(last_error.expect("终止错误已保存"));
            }
            if complete.iter().all(|done| *done) {
                break;
            }
        }
        if complete.iter().any(|done| !done) {
            return Err(
                last_error.unwrap_or_else(|| AppError::internal("腾讯云 COS 分片下载重试耗尽"))
            );
        }
        merge_parts(&parts, local_path, length).await?;
        // 默认不启用 CRC；成功合并后清理该目标的分片文件，包括先前残留分片。
        let mut files = fs::read_dir(directory).await.map_err(io_error)?;
        while let Some(file) = files.next_entry().await.map_err(io_error)? {
            if file.file_name().to_string_lossy().starts_with(&prefix)
                && !file.file_type().await.map_err(io_error)?.is_dir()
            {
                fs::remove_file(file.path()).await.map_err(io_error)?;
            }
        }
        Ok(last_status.is_success())
    }

    async fn download_part(
        &self,
        object_key: &str,
        index: usize,
        part: &DownloadPart,
        cancellation: &CancellationToken,
    ) -> (usize, Result<StatusCode, DownloadFailure>) {
        let result = async {
            let mut response = self.download_response(Method::GET, object_key, Some(&part.range), cancellation).await?;
            let status = response.status();
            let mut file = fs::File::create(&part.path).await
                .map_err(|_| DownloadFailure::client("创建腾讯云 COS 分片文件失败"))?;
            let copy = async {
                loop {
                    let chunk = tokio::select! {
                        biased;
                        _ = cancellation.cancelled() => return Err(DownloadFailure::cancelled()),
                        chunk = response.chunk() => chunk.map_err(|_| DownloadFailure::client("读取腾讯云 COS 分片失败"))?,
                    };
                    let Some(chunk) = chunk else { break };
                    file.write_all(&chunk).await.map_err(|_| DownloadFailure::client("写入腾讯云 COS 分片失败"))?;
                }
                Ok(())
            }.await;
            // 即使网络读取失败也等待 Tokio 文件后台写入结束，再允许重试覆盖。
            let flush = file.flush().await.map_err(|_| DownloadFailure::client("刷新腾讯云 COS 分片失败"));
            copy?;
            flush?;
            Ok(status)
        }.await;
        (index, result)
    }

    /// SDK 异步请求取得响应前最多重试三次；正文拷贝失败交由传输层处理。
    async fn download_response(
        &self,
        method: Method,
        object_key: &str,
        range: Option<&str>,
        cancellation: &CancellationToken,
    ) -> Result<Response, DownloadFailure> {
        let mut signed_headers = vec![("content-length", "0")];
        if let Some(range) = range {
            signed_headers.push(("range", range));
        }
        let authorization = self
            .authorization_with_headers(&method, object_key, &signed_headers)
            .map_err(|error| DownloadFailure {
                error,
                terminal: false,
            })?;
        for attempt in 1..=REQUEST_ATTEMPTS {
            let mut request = self
                .http_clients
                .raw_streaming_client()
                .request(method.clone(), self.object_url(object_key))
                .header(HOST, &self.host)
                .header(CONTENT_LENGTH, "0")
                .header(AUTHORIZATION, &authorization);
            if let Some(range) = range {
                request = request.header(RANGE, range);
            }
            let response = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(DownloadFailure::cancelled()),
                result = timeout(self.http_clients.request_timeout(), request.send()) => result,
            };
            let failure = match response {
                Ok(Ok(response)) if response.status().is_success() => return Ok(response),
                Ok(Ok(response)) => DownloadFailure {
                    error: AppError::internal(format!(
                        "腾讯云 COS 下载请求失败: HTTP {}",
                        response.status().as_u16()
                    )),
                    terminal: response.status().as_u16() < 500,
                },
                Ok(Err(_)) => DownloadFailure::client("腾讯云 COS 下载请求失败"),
                Err(_) => DownloadFailure::client("腾讯云 COS 下载请求超时"),
            };
            // HTTP 层连 4xx 也会重试；用尽后才由传输层判断不可重试的服务错误。
            if attempt == REQUEST_ATTEMPTS {
                return Err(failure);
            }
        }
        unreachable!("末次请求失败直接返回")
    }
}

async fn merge_parts(
    parts: &[DownloadPart],
    local_path: &str,
    length: i64,
) -> Result<(), AppError> {
    let existing_length = match fs::metadata(local_path).await {
        Ok(metadata) => Some(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(io_error(error)),
    };
    let truncate = existing_length.is_some_and(|value| i128::from(value) != i128::from(length));
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(truncate)
        .open(local_path)
        .await
        .map_err(io_error)?;
    let copied = async {
        for part in parts {
            let mut input = match fs::File::open(&part.path).await {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    for part in parts {
                        match fs::remove_file(Path::new(&part.path)).await {
                            Ok(()) => {}
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                            Err(error) => return Err(io_error(error)),
                        }
                    }
                    return Err(AppError::internal(
                        "腾讯云 COS 分片文件缺失，可能存在同目标并发下载",
                    ));
                }
                Err(error) => return Err(io_error(error)),
            };
            tokio::io::copy(&mut input, &mut output)
                .await
                .map_err(io_error)?;
        }
        Ok::<_, AppError>(())
    }
    .await;
    let flushed = output.flush().await.map_err(io_error);
    copied?;
    flushed?;
    if i128::from(output.metadata().await.map_err(io_error)?.len()) != i128::from(length) {
        return Err(AppError::internal(
            "腾讯云 COS 下载文件长度与对象长度不一致",
        ));
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> AppError {
    AppError::internal(format!("腾讯云 COS 下载文件操作失败: {error}"))
}
