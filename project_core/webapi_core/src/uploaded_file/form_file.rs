//! 普通上传端点的必填文件绑定，与签名共用原始正文缓存。

use axum::{
    extract::{FromRequest, Request},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use common::app_error::AppError;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{error::json_error_response, replay_body::ReplayBody};

/// 绑定名为 file 的首个文件；完整表单读取成功后才能调用 Action 过滤器。
pub struct FormFile {
    name: String,
    length: u64,
    index: usize,
    boundary: String,
    body: ReplayBody,
}

impl FormFile {
    pub fn file_name(&self) -> &str {
        &self.name
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// 只复制已绑定文件；重复同名字段不会替换最先选择的文件。
    pub async fn copy_to(mut self, writer: &mut (impl AsyncWrite + Unpin)) -> Result<(), AppError> {
        let stream = self.body.stream().await.map_err(AppError::internal)?;
        let mut multipart = multer::Multipart::new(stream, self.boundary);
        let mut index = 0;
        while let Some(mut field) = multipart.next_field().await.map_err(copy_error)? {
            if index == self.index {
                while let Some(chunk) = field.chunk().await.map_err(copy_error)? {
                    writer.write_all(&chunk).await.map_err(|error| {
                        AppError::internal(format!("写入上传文件失败: {error}"))
                    })?;
                }
                writer
                    .flush()
                    .await
                    .map_err(|error| AppError::internal(format!("刷新上传文件失败: {error}")))?;
                return Ok(());
            }
            index += 1;
        }
        Err(AppError::internal("已绑定的上传文件无法回放"))
    }
}

impl<S: Send + Sync> FromRequest<S> for FormFile {
    type Rejection = Response;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let (parts, body) = request.into_parts();
        let content_type = parts
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        // 源端没有 Content-Type 或明确没有正文时不读取表单，交给必填验证。
        if content_type.is_empty()
            || parts
                .headers
                .get(header::CONTENT_LENGTH)
                .is_some_and(|value| value == "0")
        {
            return Err(missing_file());
        }
        let boundary = multer::parse_boundary(content_type).map_err(form_error)?;
        let mut body = parts
            .extensions
            .get::<ReplayBody>()
            .cloned()
            .unwrap_or_else(|| ReplayBody::new(body));
        let stream = body
            .stream()
            .await
            .map_err(|error| crate::error::internal_error_response(&error))?;
        let mut multipart = multer::Multipart::new(stream, boundary.clone());
        let mut selected = None;
        let mut index = 0;
        while let Some(mut field) = multipart.next_field().await.map_err(form_error)? {
            let disposition =
                crate::form_disposition::read(field.headers()).map_err(form_message)?;
            let name = disposition
                .as_ref()
                .and_then(crate::form_disposition::file_name)
                .map(str::to_owned);
            let matches = disposition
                .as_ref()
                .and_then(|value| value.name.as_deref())
                .is_some_and(|name| name.eq_ignore_ascii_case("file"));
            let mut length = 0;
            while let Some(chunk) = field.chunk().await.map_err(form_error)? {
                length += chunk.len() as u64;
            }
            if selected.is_none()
                && matches
                && let Some(name) = name
            {
                selected = Some((name, length, index));
            }
            index += 1;
        }
        let (name, length, index) = selected.ok_or_else(missing_file)?;
        Ok(Self {
            name,
            length,
            index,
            boundary,
            body,
        })
    }
}

/// 普通文件端点的媒体类型选择先于认证；空类型继续进入参数绑定。
/// 通过 route_layer 装配，不能拦截其他 HTTP 方法的 405。
pub async fn require_multipart(request: Request, next: Next) -> Response {
    if let Some(value) = request.headers().get(header::CONTENT_TYPE)
        && !value.is_empty()
        && !value.to_str().ok().is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case("multipart/form-data")
        })
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    next.run(request).await
}

fn missing_file() -> Response {
    json_error_response(
        StatusCode::BAD_REQUEST,
        "file : The file field is required.",
    )
}

fn form_error(error: multer::Error) -> Response {
    let message = match error {
        multer::Error::NoBoundary => "Missing content-type boundary.",
        multer::Error::IncompleteStream
        | multer::Error::IncompleteHeaders
        | multer::Error::IncompleteFieldData { .. } => {
            "Unexpected end of Stream, the content may have already been read by another component. "
        }
        multer::Error::NoMultipart | multer::Error::DecodeContentType(_) => {
            return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
        }
        _ => return crate::error::internal_error_response(&error.to_string()),
    };
    form_message(message)
}

fn form_message(message: impl std::fmt::Display) -> Response {
    json_error_response(
        StatusCode::BAD_REQUEST,
        format!(" : Failed to read the request form. {message}"),
    )
}

fn copy_error(error: multer::Error) -> AppError {
    AppError::internal(format!("回放上传文件失败: {error}"))
}
