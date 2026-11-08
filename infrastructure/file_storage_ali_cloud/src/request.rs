use std::{
    collections::BTreeMap,
    future::Future,
    io::Cursor,
    sync::{Arc, Mutex, atomic::Ordering},
    time::Duration,
};

use common::app_error::AppError;
use futures_util::TryStreamExt;
use reqwest::{Method, StatusCode, header::HeaderMap};
use tokio::{
    io::AsyncRead,
    time::{Instant, sleep, timeout_at},
};
use tokio_util::{io::StreamReader, sync::CancellationToken};

use super::{
    AliyunOssFileStorage, READ_WRITE_TIMEOUT, integrity::TransferChecksum,
    service_error::ServiceError, upload::UploadSource,
};

const REQUEST_ATTEMPTS: usize = 3;

/// 请求层交付的正文：普通 GET 保持流式，203 已在请求期限内完整缓冲。
pub(super) struct ObjectResponse {
    pub(super) status: StatusCode,
    pub(super) headers: HeaderMap,
    pub(super) body: Box<dyn AsyncRead + Unpin + Send>,
}

#[derive(Clone, Copy)]
struct DeadlineState {
    read_write: Instant,
    client_completed: bool,
}

/// 单次请求的读写期限；大文件上传读取有进度时向后移动。
#[derive(Clone)]
pub(super) struct RequestDeadline(Arc<Mutex<DeadlineState>>);

impl RequestDeadline {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(DeadlineState {
            read_write: Instant::now() + READ_WRITE_TIMEOUT,
            client_completed: false,
        })))
    }

    pub(super) fn progress(&self) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .read_write = Instant::now() + READ_WRITE_TIMEOUT;
    }

    /// ResponseHeadersRead 的 HttpClient 期限到响应头为止，不延伸到 SDK 缓冲。
    fn complete_client(&self) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .client_completed = true;
    }

    fn current(&self) -> DeadlineState {
        *self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    async fn run<T>(
        &self,
        client_timeout: Duration,
        operation: impl Future<Output = Result<T, RequestFailure>>,
    ) -> Result<T, RequestFailure> {
        let client_deadline = Instant::now() + client_timeout;
        let mut operation = std::pin::pin!(operation);
        loop {
            let state = self.current();
            let deadline = if state.client_completed {
                state.read_write
            } else {
                state.read_write.min(client_deadline)
            };
            match timeout_at(deadline, operation.as_mut()).await {
                Ok(result) => return result,
                Err(_) => {
                    let now = Instant::now();
                    let state = self.current();
                    if !state.client_completed
                        && client_deadline <= state.read_write
                        && now >= client_deadline
                    {
                        // 源 HttpClient 自己的整包期限不是 SDK 的可重试读写超时。
                        return Err(RequestFailure {
                            error: AppError::internal("阿里云 OSS 请求超过 HTTP 客户端期限"),
                            retryable: false,
                        });
                    }
                    if now >= state.read_write {
                        return Err(RequestFailure {
                            error: AppError::internal("阿里云 OSS 请求读写超时"),
                            retryable: true,
                        });
                    }
                    // 上传推进或 GET 收到响应头后旧计时器到期，继续同一请求。
                }
            }
        }
    }
}

struct RequestFailure {
    error: AppError,
    retryable: bool,
}

impl RequestFailure {
    fn transport(error: reqwest::Error) -> Self {
        Self {
            retryable: !error.is_builder(),
            error: AppError::internal("阿里云 OSS 网络请求失败"),
        }
    }
}

impl AliyunOssFileStorage {
    /// 请求层重试与下载文件拷贝层分别计数，复用源 SDK 的两层边界。
    pub(super) async fn execute_request(
        &self,
        method: Method,
        object_key: &str,
        headers: BTreeMap<&str, String>,
        upload: Option<&UploadSource>,
    ) -> Result<ObjectResponse, AppError> {
        let mut clock_offset = self.clock_offset.load(Ordering::Relaxed);
        for attempt in 1..=REQUEST_ATTEMPTS {
            let deadline = RequestDeadline::new();
            let cancellation = CancellationToken::new();
            let cancel_on_drop = cancellation.clone().drop_guard();
            let checksum = TransferChecksum::new();
            let mut request =
                self.build_request(method.clone(), object_key, headers.clone(), clock_offset)?;
            if let Some(source) = upload {
                request = request.body(
                    source
                        .body(checksum.clone(), deadline.clone(), cancellation)
                        .await?,
                );
            }
            deadline.progress();
            let result = deadline
                .run(self.http_clients.request_timeout(), async {
                    let mut response = request.send().await.map_err(RequestFailure::transport)?;
                    if method == Method::GET {
                        deadline.complete_client();
                    }
                    let status = response.status();
                    if !status.is_success() {
                        let response_headers = response.headers().clone();
                        let body = response.bytes().await.map_err(RequestFailure::transport)?;
                        let service_error =
                            ServiceError::parse(&body, &response_headers).map_err(|()| {
                                RequestFailure {
                                    error: AppError::internal("阿里云 OSS 错误响应编码无效"),
                                    retryable: false,
                                }
                            })?;
                        if let Some(offset) = service_error.clock_offset(status, &response_headers)
                        {
                            clock_offset = offset;
                            let cached = self.clock_offset.load(Ordering::Relaxed);
                            if offset.abs_diff(cached) > 60 {
                                self.clock_offset.store(offset, Ordering::Relaxed);
                            }
                        }
                        return Err(RequestFailure {
                            error: AppError::internal(format!(
                                "阿里云 OSS 请求失败: HTTP {}",
                                status.as_u16()
                            )),
                            retryable: service_error.retryable(status),
                        });
                    }
                    if method != Method::GET {
                        // Put/Delete 使用 ResponseContentRead；消费完响应再检查结果。
                        while response
                            .chunk()
                            .await
                            .map_err(RequestFailure::transport)?
                            .is_some()
                        {}
                        if upload.is_some() {
                            checksum.verify(response.headers()).map_err(|error| {
                                RequestFailure {
                                    error,
                                    retryable: true,
                                }
                            })?;
                        }
                    }
                    let headers = response.headers().clone();
                    let body: Box<dyn AsyncRead + Unpin + Send> = if method == Method::GET
                        && status == StatusCode::NON_AUTHORITATIVE_INFORMATION
                    {
                        // SDK 在交给文件拷贝层之前将 203 读入可定位的内存流。
                        // 缓冲失败属于请求层重试，不能提前创建或截断本地文件。
                        Box::new(Cursor::new(
                            response.bytes().await.map_err(RequestFailure::transport)?,
                        ))
                    } else {
                        Box::new(StreamReader::new(
                            response.bytes_stream().map_err(std::io::Error::other),
                        ))
                    };
                    Ok(ObjectResponse {
                        status,
                        headers,
                        body,
                    })
                })
                .await;
            // 包括超时在内的所有路径均停止旧请求的文件读取，再进入下一轮。
            drop(cancel_on_drop);
            match result {
                Ok(response) => return Ok(response),
                Err(failure) if !failure.retryable || attempt == REQUEST_ATTEMPTS => {
                    return Err(failure.error);
                }
                Err(_) => {
                    // 0.3.0 StandardRetryer 将 MaxBackOff/BaseDelay 反序传给
                    // FullJitterBackoff，默认实际延迟为 [0, 200ms)，按实现对齐。
                    sleep(Duration::from_secs_f64(rand::random::<f64>() * 0.2)).await;
                }
            }
        }
        unreachable!("请求次数固定大于零，末次失败直接返回")
    }
}
