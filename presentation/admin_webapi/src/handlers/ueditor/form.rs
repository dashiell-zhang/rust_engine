//! UEditor 的两种表单编码共用读取入口；完整读取成功后才执行文件服务。
use std::{path::Path, sync::Arc};

use axum::extract::{FromRequest, Multipart, Request};
use common::io_helper::TemporaryFileGuard;
use futures_util::StreamExt;
use tokio::{fs, io::AsyncWriteExt};

use crate::state::AppState;

const VALUE_COUNT_LIMIT: usize = 1024;
const KEY_LENGTH_LIMIT: usize = 2048;
const VALUE_LENGTH_LIMIT: usize = 4 * 1024 * 1024;

/// 保留重复普通字段的原始顺序，以及指定名称的第一个文件。
#[derive(Default)]
pub(super) struct Form {
    fields: Vec<(String, String)>,
    pub file: Option<UploadFile>,
}

impl Form {
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|(key, _)| common::string_helper::eq_ordinal_ignore_case(key, name))
            .map(|(_, value)| value.as_str())
            .collect()
    }
}

/// 临时文件所有权覆盖完整表单读取、业务保存与取消。
pub(super) struct UploadFile {
    pub name: String,
    pub length: usize,
    pub temp: TemporaryFileGuard,
}

pub(super) async fn read(
    request: Request,
    state: &Arc<AppState>,
    file_field: Option<&str>,
) -> Result<Form, String> {
    let content_type = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let media_type = content_type.split(';').next().unwrap_or_default().trim();
    let multipart = media_type.eq_ignore_ascii_case("multipart/form-data");
    if !multipart && !media_type.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
        return Err(format!("Incorrect Content-Type: {content_type}"));
    }
    if request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .is_some_and(|value| value == "0")
    {
        return Ok(Form::default());
    }
    if !multipart {
        return read_urlencoded(request).await;
    }
    let mut multipart = Multipart::from_request(request, state)
        .await
        .map_err(|error| error.to_string())?;
    let mut form = Form::default();
    let mut count = 0;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|error| error.to_string())?
    {
        count += 1;
        check_count(count)?;
        let name = field.name().unwrap_or_default().to_string();
        if let Some(file_name) = field.file_name().map(str::to_owned) {
            if form.file.is_none() && file_field.is_some_and(|key| key.eq_ignore_ascii_case(&name))
            {
                let temp_dir = Path::new(&state.settings.file.root_path).join("temps");
                fs::create_dir_all(&temp_dir)
                    .await
                    .map_err(|error| error.to_string())?;
                let path = super::build_temp_upload_path(&temp_dir, &file_name);
                let temp = TemporaryFileGuard::new(&path);
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .await
                    .map_err(|error| error.to_string())?;
                let mut length = 0;
                while let Some(chunk) = field.chunk().await.map_err(|error| error.to_string())? {
                    length += chunk.len();
                    output
                        .write_all(&chunk)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                output.flush().await.map_err(|error| error.to_string())?;
                form.file = Some(UploadFile {
                    name: file_name,
                    length,
                    temp,
                });
            } else {
                // 不使用的文件也必须读到结尾，不能将残缺表单当作成功请求。
                while field
                    .chunk()
                    .await
                    .map_err(|error| error.to_string())?
                    .is_some()
                {}
            }
        } else {
            // ASP.NET 的 multipart 普通字段不套用 URL 编码表单的 4 MiB 限制。
            let value = field.text().await.map_err(|error| error.to_string())?;
            form.fields.push((name, value));
        }
    }
    Ok(form)
}

/// 按字段增量读取 URL 编码表单，限制原始编码长度，不限制整个请求为 10 MiB。
async fn read_urlencoded(request: Request) -> Result<Form, String> {
    let mut stream = request.into_body().into_data_stream();
    let mut pending = Vec::new();
    let mut form = Form::default();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        let mut remaining = chunk.as_ref();
        while let Some(end) = remaining.iter().position(|value| *value == b'&') {
            pending.extend_from_slice(&remaining[..end]);
            append_pair(&mut form, &pending)?;
            pending.clear();
            remaining = &remaining[end + 1..];
        }
        pending.extend_from_slice(remaining);
        if pending.len() > KEY_LENGTH_LIMIT + VALUE_LENGTH_LIMIT + 2 {
            return Err("Form key or value length limit exceeded.".into());
        }
    }
    if !pending.is_empty() {
        append_pair(&mut form, &pending)?;
    }
    Ok(form)
}

fn append_pair(form: &mut Form, pair: &[u8]) -> Result<(), String> {
    let (key_length, value_length) = pair
        .iter()
        .position(|value| *value == b'=')
        .map(|position| (position, pair.len() - position - 1))
        .unwrap_or((pair.len(), 0));
    if key_length > KEY_LENGTH_LIMIT {
        return Err(format!(
            "Form key length limit {KEY_LENGTH_LIMIT} exceeded."
        ));
    }
    if value_length > VALUE_LENGTH_LIMIT {
        return Err(format!(
            "Form value length limit {VALUE_LENGTH_LIMIT} exceeded."
        ));
    }
    check_count(form.fields.len() + 1)?;
    // 空段也是一个空键值，保持 FormPipeReader 的计数语义。
    if pair.is_empty() {
        form.fields.push((String::new(), String::new()));
    } else {
        let mut pairs = serde_urlencoded::from_bytes::<Vec<(String, String)>>(pair)
            .map_err(|error| error.to_string())?;
        form.fields.append(&mut pairs);
    }
    Ok(())
}

fn check_count(count: usize) -> Result<(), String> {
    if count > VALUE_COUNT_LIMIT {
        return Err(format!(
            "Form value count limit {VALUE_COUNT_LIMIT} exceeded."
        ));
    }
    Ok(())
}
