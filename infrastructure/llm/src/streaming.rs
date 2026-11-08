use futures_util::{StreamExt, stream::BoxStream};

use crate::{LLM_REQUEST_TIMEOUT, LlmError};

mod decoder;

/// 流式请求只限制等待响应头的时间，正文由调用方消费或取消。
pub(crate) async fn send(request: reqwest::RequestBuilder) -> Result<reqwest::Response, LlmError> {
    tokio::time::timeout(LLM_REQUEST_TIMEOUT, request.send())
        .await
        .map_err(|_| LlmError::Http("LLM response headers timed out".into()))?
        .map_err(Into::into)
}

/// 先增量解码再按字符拆行；兼容 BOM、LF、CRLF、CR 和末行无换行。
pub(crate) fn lines(response: reqwest::Response) -> BoxStream<'static, Result<String, LlmError>> {
    Box::pin(async_stream::try_stream! {
        let mut stream = text_chunks(response);
        let mut buffer = String::new();
        let mut after_cr = false;
        while let Some(chunk) = stream.next().await {
            for character in chunk?.chars() {
                if after_cr {
                    after_cr = false;
                    // CR 位于读取块末尾时，源 ReadLineAsync 会先等待下一字符或 EOF。
                    yield std::mem::take(&mut buffer);
                    if character == '\n' { continue; }
                }
                if character == '\r' {
                    after_cr = true;
                } else if character == '\n' {
                    yield std::mem::take(&mut buffer);
                } else {
                    buffer.push(character);
                }
            }
        }
        if after_cr || !buffer.is_empty() {
            yield buffer;
        }
    })
}

fn text_chunks(response: reqwest::Response) -> BoxStream<'static, Result<String, LlmError>> {
    Box::pin(async_stream::try_stream! {
        let mut stream = response.bytes_stream();
        let mut decoder = decoder::TextDecoder::default();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let mut remaining = chunk.as_ref();
            while !remaining.is_empty() {
                let count = decoder.read_capacity().min(remaining.len());
                let text = decoder.read(&remaining[..count]);
                remaining = &remaining[count..];
                if !text.is_empty() {
                    yield text;
                }
            }
        }
        let text = decoder.finish();
        if !text.is_empty() {
            yield text;
        }
    })
}
