//! 读取时缓存请求体；各回放流拥有独立偏移，超过阈值使用匿名临时文件。
use std::{io, pin::Pin, sync::Arc};

use axum::body::{Body, Bytes};
use futures_util::{Stream, StreamExt, stream};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::Mutex,
};

const MEMORY_THRESHOLD: usize = 30 * 1024;
type BodyStream<'a> = Pin<Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send + 'a>>;

#[derive(Clone)]
pub(crate) struct ReplayBody(Arc<Mutex<BufferedBody>>);

struct BufferedBody {
    source: BodyStream<'static>,
    storage: Storage,
    length: u64,
    eof: bool,
    failure: Option<String>,
}

enum Storage {
    Memory(Vec<u8>),
    File(tokio::fs::File),
}

impl ReplayBody {
    /// 尚不消费网络正文；参数提取或过滤器首次读取时才填充缓存。
    pub(crate) fn new(body: Body) -> Self {
        Self(Arc::new(Mutex::new(BufferedBody {
            source: Box::pin(body.into_data_stream().map(|chunk| {
                chunk.map_err(|error| io::Error::other(format!("请求体读取失败: {error}")))
            })),
            storage: Storage::Memory(Vec::new()),
            length: 0,
            eof: false,
            failure: None,
        })))
    }

    /// 没有预先绑定参数的调用点可以主动填充完整缓存，不把文件整体读入内存。
    pub(crate) async fn capture(body: Body) -> Result<Self, String> {
        let mut buffered = Self::new(body);
        let mut stream = buffered.stream().await?;
        while let Some(chunk) = stream.next().await {
            chunk.map_err(|error| error.to_string())?;
        }
        drop(stream);
        Ok(buffered)
    }

    pub(crate) async fn stream(&mut self) -> Result<BodyStream<'static>, String> {
        Ok(replay_stream(self.0.clone()))
    }

    /// 文本与普通表单需要完整字节；文件表单使用 stream。
    pub(crate) async fn bytes(&mut self) -> Result<Bytes, String> {
        let mut stream = self.stream().await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk.map_err(|error| error.to_string())?);
        }
        Ok(Bytes::from(bytes))
    }

    pub(crate) async fn into_body(self) -> Result<Body, String> {
        Ok(Body::from_stream(replay_stream(self.0)))
    }

    pub(crate) async fn failure(&self) -> Option<String> {
        self.0.lock().await.failure.clone()
    }
}

impl BufferedBody {
    async fn next(&mut self, offset: u64) -> io::Result<Option<Bytes>> {
        if offset < self.length {
            let count = (self.length - offset).min(64 * 1024) as usize;
            let chunk = match &mut self.storage {
                Storage::Memory(bytes) => {
                    Bytes::copy_from_slice(&bytes[offset as usize..offset as usize + count])
                }
                Storage::File(file) => {
                    file.seek(io::SeekFrom::Start(offset)).await?;
                    let mut bytes = vec![0; count];
                    file.read_exact(&mut bytes).await?;
                    Bytes::from(bytes)
                }
            };
            return Ok(Some(chunk));
        }
        if let Some(error) = &self.failure {
            return Err(io::Error::other(error.clone()));
        }
        if self.eof {
            return Ok(None);
        }
        let Some(chunk) = self.source.next().await else {
            self.eof = true;
            return Ok(None);
        };
        let chunk = chunk?;
        if let Storage::Memory(memory) = &self.storage
            && memory.len().saturating_add(chunk.len()) > MEMORY_THRESHOLD
        {
            // 只使用指定临时目录，最后一个句柄关闭后由系统回收匿名文件。
            let file = match std::env::var_os("ASPNETCORE_TEMP") {
                Some(directory) if !directory.is_empty() => tempfile::tempfile_in(directory),
                _ => tempfile::tempfile(),
            }?;
            let mut file = tokio::fs::File::from_std(file);
            file.write_all(memory).await?;
            self.storage = Storage::File(file);
        }
        match &mut self.storage {
            Storage::Memory(memory) => memory.extend_from_slice(&chunk),
            Storage::File(file) => {
                file.seek(io::SeekFrom::Start(self.length)).await?;
                file.write_all(&chunk).await?;
                file.flush().await?;
            }
        }
        self.length += chunk.len() as u64;
        Ok(Some(chunk))
    }
}

fn replay_stream(buffered: Arc<Mutex<BufferedBody>>) -> BodyStream<'static> {
    Box::pin(stream::try_unfold(
        (buffered, 0_u64),
        |(buffered, offset)| async move {
            let mut reader = buffered.lock().await;
            let result = reader.next(offset).await;
            if let Err(error) = &result {
                reader.failure = Some(error.to_string());
            }
            drop(reader);
            match result? {
                Some(chunk) => {
                    let next = offset + chunk.len() as u64;
                    Ok(Some((chunk, (buffered, next))))
                }
                None => Ok(None),
            }
        },
    ))
}
