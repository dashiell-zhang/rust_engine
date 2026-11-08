use std::{collections::BTreeMap, io::SeekFrom};

use base64::{Engine, engine::general_purpose::STANDARD};
use common::app_error::AppError;
use md5::{Digest, Md5};
use reqwest::{
    Body, Method, StatusCode,
    header::{AUTHORIZATION, HOST, HeaderMap},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt},
};
use tokio_util::io::ReaderStream;

use super::{TencentCosFileStorage, cos_percent_encode};

const REQUEST_ATTEMPTS: usize = 4;
const BUFFER_SIZE: usize = 64 * 1024;

pub(super) enum RequestBody<'a> {
    Empty,
    Bytes(Vec<u8>),
    File {
        path: &'a str,
        offset: u64,
        length: u64,
    },
}

pub(super) struct BufferedResponse {
    pub(super) status: StatusCode,
    pub(super) headers: HeaderMap,
    pub(super) body: Vec<u8>,
}

impl RequestBody<'_> {
    async fn length_and_digest(&mut self) -> Result<(u64, Option<String>), AppError> {
        match self {
            Self::Empty => Ok((0, None)),
            Self::Bytes(bytes) => Ok((
                bytes.len() as u64,
                Some(STANDARD.encode(Md5::digest(bytes))),
            )),
            Self::File {
                path,
                offset,
                length,
            } => {
                let available = fs::metadata(*path)
                    .await
                    .map_err(file_error)?
                    .len()
                    .checked_sub(*offset)
                    .ok_or_else(|| AppError::internal("腾讯云 COS 上传起点超出文件长度"))?;
                *length = (*length).min(available);
                Ok((
                    *length,
                    Some(STANDARD.encode(file_digest(path, *offset, *length).await?)),
                ))
            }
        }
    }

    async fn open(&self) -> Result<Option<Body>, AppError> {
        Ok(match self {
            Self::Empty => None,
            Self::Bytes(bytes) => Some(Body::from(bytes.clone())),
            Self::File {
                path,
                offset,
                length,
            } => {
                // SDK 每次发送重新打开路径并定位，不把前次失败后的偏移带入重试。
                let mut file = fs::File::open(path).await.map_err(file_error)?;
                file.seek(SeekFrom::Start(*offset))
                    .await
                    .map_err(file_error)?;
                Some(Body::wrap_stream(ReaderStream::with_capacity(
                    file.take(*length),
                    BUFFER_SIZE,
                )))
            }
        })
    }
}

pub(super) async fn file_digest(
    path: &str,
    offset: u64,
    length: u64,
) -> Result<[u8; 16], AppError> {
    let mut file = fs::File::open(path).await.map_err(file_error)?;
    file.seek(SeekFrom::Start(offset))
        .await
        .map_err(file_error)?;
    let mut file = file.take(length);
    let mut digest = Md5::new();
    let mut buffer = vec![0; BUFFER_SIZE];
    loop {
        let read = file.read(&mut buffer).await.map_err(file_error)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest.finalize().into())
}

impl TencentCosFileStorage {
    /// SDK 的缓冲响应请求：签入元数据、查询参数与正文 MD5，重试复用签名。
    pub(super) async fn execute_request(
        &self,
        method: Method,
        object_key: &str,
        query: &[(&str, &str)],
        mut headers: BTreeMap<&str, String>,
        mut body: RequestBody<'_>,
    ) -> Result<BufferedResponse, AppError> {
        let (length, digest) = body.length_and_digest().await?;
        headers.insert("content-length", length.to_string());
        if let Some(digest) = digest {
            headers.insert("content-md5", digest);
        }
        let signed_headers: Vec<_> = headers
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        let authorization =
            self.request_authorization(&method, object_key, &signed_headers, query)?;
        let mut url = self.object_url(object_key);
        if !query.is_empty() {
            url.push('?');
            url.push_str(
                &query
                    .iter()
                    .map(|(key, value)| {
                        format!("{}={}", cos_percent_encode(key), cos_percent_encode(value))
                    })
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        for attempt in 1..=REQUEST_ATTEMPTS {
            let mut request = self
                .http_clients
                .raw_streaming_client()
                .request(method.clone(), &url)
                .timeout(self.http_clients.request_timeout())
                .header(HOST, &self.host)
                .header(AUTHORIZATION, &authorization);
            for (name, value) in &headers {
                request = request.header(*name, value);
            }
            match body.open().await {
                Ok(Some(body)) => request = request.body(body),
                Ok(None) => {}
                Err(error) if attempt == REQUEST_ATTEMPTS => return Err(error),
                Err(_) => continue,
            }
            match request.send().await {
                Ok(response) if response.status().is_success() => {
                    let status = response.status();
                    let headers = response.headers().clone();
                    // SDK 正文读取或 XML 解析失败直接交付错误，不再次发送已完成的写请求。
                    let body = response
                        .bytes()
                        .await
                        .map_err(|_| AppError::internal("读取腾讯云 COS 响应失败"))?
                        .to_vec();
                    return Ok(BufferedResponse {
                        status,
                        headers,
                        body,
                    });
                }
                Ok(response) if attempt == REQUEST_ATTEMPTS => {
                    return Err(AppError::internal(format!(
                        "腾讯云 COS 请求失败: HTTP {}",
                        response.status().as_u16()
                    )));
                }
                Err(_) if attempt == REQUEST_ATTEMPTS => {
                    return Err(AppError::internal("腾讯云 COS 请求失败"));
                }
                _ => {}
            }
        }
        unreachable!("末次失败直接返回")
    }
}

fn file_error(error: std::io::Error) -> AppError {
    AppError::internal(format!("腾讯云 COS 上传文件操作失败: {error}"))
}
