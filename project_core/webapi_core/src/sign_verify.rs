use std::convert::Infallible;

use axum::{
    Json,
    body::{Body, Bytes, to_bytes},
    extract::{Request, State},
    http::{StatusCode, header::CONTENT_TYPE},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::MethodRouter,
};
use chrono::Utc;
use futures_util::stream;
use multer::Multipart;
use sha2::{Digest, Sha256};

use crate::auth::HasJwtCodec;
use crate::error::ErrBody;

/// 为单个路由挂载请求签名验证中间件
pub fn sign_verify_method_router<TState>(
    route: MethodRouter<std::sync::Arc<TState>>,
    state: std::sync::Arc<TState>,
) -> MethodRouter<std::sync::Arc<TState>>
where
    TState: HasJwtCodec + Send + Sync + 'static,
{
    route.layer(from_fn_with_state(state, sign_verify_middleware::<TState>))
}

/// 请求签名验证中间件
///
/// 签名原文格式与 NetEngine 保持一致
///
/// JWT签名段 + Time 请求头 + 请求路径和查询串 + 请求体签名内容
pub async fn sign_verify_middleware<TState>(
    State(state): State<std::sync::Arc<TState>>,
    request: Request,
    next: Next,
) -> Response
where
    TState: HasJwtCodec + Send + Sync + 'static,
{
    if cfg!(debug_assertions) {
        return next.run(request).await;
    }

    let token_header = match request
        .headers()
        .get("Token")
        .and_then(|value| value.to_str().ok())
    {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => return unauthorized("缺少 Token 请求头"),
    };

    let time_header = match request
        .headers()
        .get("Time")
        .and_then(|value| value.to_str().ok())
    {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => return unauthorized("缺少 Time 请求头"),
    };

    let time_millis = match time_header.parse::<i64>() {
        Ok(value) => value,
        Err(_) => return unauthorized("Time 请求头无效"),
    };

    let now_millis = Utc::now().timestamp_millis();
    if (now_millis - time_millis).abs() > 3 * 60 * 1000 {
        return unauthorized("Token 已过期");
    }

    let auth_token = match request
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => return unauthorized("缺少 Authorization 请求头"),
    };

    let encoded_signature = match state.jwt_codec().encoded_signature(&auth_token) {
        Some(value) if !value.is_empty() => value,
        _ => return unauthorized("Authorization 格式错误"),
    };

    let (parts, body) = request.into_parts();
    let body_bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => return unauthorized(&format!("请求体读取失败: {error}")),
    };

    let signature_body = match build_signature_body(
        parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        &body_bytes,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return unauthorized(&error),
    };

    let request_path = match parts.uri.path_and_query() {
        Some(value) => value.as_str().to_string(),
        None => parts.uri.path().to_string(),
    };

    let source = format!("{encoded_signature}{time_header}{request_path}{signature_body}");
    let calculated_token = sha256_hex(source.as_bytes());

    if token_header != calculated_token {
        return unauthorized("非法 Token");
    }

    let request = Request::from_parts(parts, Body::from(body_bytes));
    next.run(request).await
}

/// 生成参与签名的请求体字符串
///
/// 普通 JSON 与文本请求直接使用原始 body
/// 表单请求按字段名排序后依次拼接键值
/// multipart 请求还会对文件内容做 SHA-256 摘要
async fn build_signature_body(
    content_type: Option<&str>,
    body_bytes: &Bytes,
) -> Result<String, String> {
    let Some(content_type) = content_type else {
        return Ok(String::from_utf8_lossy(body_bytes).into_owned());
    };

    if content_type.starts_with("application/x-www-form-urlencoded") {
        let mut fields = url::form_urlencoded::parse(body_bytes)
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        fields.sort_by(|left, right| left.0.cmp(&right.0));

        return Ok(fields
            .into_iter()
            .fold(String::new(), |mut output, (key, value)| {
                output.push_str(&key);
                output.push_str(&value);
                output
            }));
    }

    if content_type.starts_with("multipart/form-data") {
        return build_multipart_signature_body(content_type, body_bytes).await;
    }

    Ok(String::from_utf8_lossy(body_bytes).into_owned())
}

/// 解析 multipart 请求并按 NetEngine 规则拼接签名体
async fn build_multipart_signature_body(
    content_type: &str,
    body_bytes: &Bytes,
) -> Result<String, String> {
    let boundary = multer::parse_boundary(content_type)
        .map_err(|error| format!("multipart boundary 解析失败: {error}"))?;
    let body_stream = stream::once(async move { Ok::<Bytes, Infallible>(body_bytes.clone()) });
    let mut multipart = Multipart::new(body_stream, boundary);
    let mut fields = Vec::new();
    let mut files = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| format!("multipart 读取失败: {error}"))?
    {
        let name = field.name().unwrap_or_default().to_string();

        if field.file_name().is_some() {
            let bytes = field
                .bytes()
                .await
                .map_err(|error| format!("multipart 文件读取失败: {error}"))?;
            files.push((name, sha256_hex(bytes.as_ref())));
        } else {
            let value = field
                .text()
                .await
                .map_err(|error| format!("multipart 字段读取失败: {error}"))?;
            fields.push((name, value));
        }
    }

    fields.sort_by(|left, right| left.0.cmp(&right.0));
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut output = String::new();
    for (key, value) in fields {
        output.push_str(&key);
        output.push_str(&value);
    }
    for (name, hash) in files {
        output.push_str(&name);
        output.push_str(&hash);
    }

    Ok(output)
}

/// 计算 SHA-256 十六进制摘要
fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let hash = hasher.finalize();

    hash.iter().map(|byte| format!("{byte:02X}")).collect()
}

/// 返回 401 未授权响应
fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrBody {
            err_msg: message.to_string(),
        }),
    )
        .into_response()
}
