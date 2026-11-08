use crate::routes::MethodRouter;
use axum::{
    body::Bytes,
    extract::{Request, State},
    http::{
        StatusCode,
        header::{CONTENT_ENCODING, CONTENT_TYPE},
    },
    middleware::{Next, from_fn_with_state},
    response::Response,
};
use chrono::Utc;
use futures_util::Stream;
use multer::Multipart;
use sha2::{Digest, Sha256};

use crate::auth::HasJwtCodec;
use crate::error::{internal_error_response, json_error_response};
use crate::replay_body::ReplayBody;

/// 为单个路由挂载请求签名验证中间件
pub fn sign_verify_method_router<TState>(
    route: MethodRouter<std::sync::Arc<TState>>,
    state: std::sync::Arc<TState>,
) -> MethodRouter<std::sync::Arc<TState>>
where
    TState: HasJwtCodec + Send + Sync + 'static,
{
    route.map_action(|route| {
        route.route_layer(from_fn_with_state(state, sign_verify_middleware::<TState>))
    })
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

    let headers = request.headers();
    let header_text = |name| {
        common::string_helper::join_string_values(
            headers
                .get_all(name)
                .iter()
                .filter_map(|value| value.to_str().ok()),
        )
    };
    let token_header = header_text("Token");
    let time_header = header_text("Time");

    let time_millis = match time_header.trim().parse::<i64>() {
        // DateTimeOffset.FromUnixTimeMilliseconds 及随后 AddMinutes(3) 的有效范围。
        Ok(value) if (-62_135_596_800_000..=253_402_300_619_999).contains(&value) => value,
        Ok(_) | Err(_) => return internal_error_response("Time 请求头不是有效的 Unix 毫秒时间"),
    };

    let now_millis = Utc::now().timestamp_millis();
    // 源过滤器只检查 Time + 3 分钟是否仍在未来，不设置未来时间上限。
    // 使用宽整数避免请求中的极端值触发减法或绝对值溢出。
    if i128::from(time_millis) + 3 * 60 * 1000 <= i128::from(now_millis) {
        return unauthorized("Token 已过期");
    }

    let auth_token = header_text("Authorization").replace("Bearer ", "");

    // 源过滤器读取 JsonWebToken，只解析头和载荷，不在匿名签名接口执行 JWT 验签。
    if !is_readable_jwt(&auth_token) {
        return internal_error_response("Authorization 中的 JWT 格式无效");
    }

    let encoded_signature = match state.jwt_codec().encoded_signature(&auth_token) {
        Some(value) => value,
        None => return internal_error_response("Authorization 中的 JWT 格式无效"),
    };

    let (parts, body) = request.into_parts();
    let replay = parts.extensions.get::<ReplayBody>().cloned();
    let mut body = match match replay {
        Some(body) => Ok(body),
        None => ReplayBody::capture(body).await,
    } {
        Ok(body) => body,
        Err(error) => return internal_error_response(&error),
    };

    let signature_body = match build_signature_body(
        parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        parts
            .headers
            .get(CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok()),
        &mut body,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return internal_error_response(&error),
    };

    let request_path = crate::request_path::path_and_query(&parts.uri, &parts.extensions);

    let mut hasher = Sha256::new();
    for part in [
        encoded_signature,
        &time_header,
        &request_path,
        &signature_body,
    ] {
        hasher.update(part.as_bytes());
    }
    let calculated_token = digest_hex(&hasher.finalize());

    if token_header != calculated_token {
        return unauthorized("非法 Token");
    }

    let body = match body.into_body().await {
        Ok(body) => body,
        Err(error) => return internal_error_response(&error),
    };
    let request = Request::from_parts(parts, body);
    next.run(request).await
}

fn is_readable_jwt(token: &str) -> bool {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let parts: Vec<_> = token.split('.').collect();
    parts.len() == 3
        && parts[..2].iter().all(|part| {
            URL_SAFE_NO_PAD
                .decode(part)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .is_some_and(|value| value.is_object())
        })
}

/// 生成参与签名的请求体字符串
///
/// 普通 JSON 与文本请求直接使用原始 body
/// 表单请求按字段名排序后依次拼接键值
/// multipart 请求还会对文件内容做 SHA-256 摘要
async fn build_signature_body(
    content_type: Option<&str>,
    content_encoding: Option<&str>,
    body: &mut ReplayBody,
) -> Result<String, String> {
    let content_type = content_type.unwrap_or_default();
    let media_type = content_type.split(';').next().unwrap_or_default().trim();

    if media_type.eq_ignore_ascii_case("multipart/form-data") {
        return build_multipart_signature_body(content_type, body.stream().await?).await;
    }
    let body_bytes = body.bytes().await?;
    if media_type.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
        let mut fields = Vec::new();
        for (key, value) in url::form_urlencoded::parse(&body_bytes) {
            crate::request_parameters::append_joined(
                &mut fields,
                key.into_owned(),
                value.into_owned(),
            );
        }
        return form_signature_body(fields);
    }

    crate::request_body::read_signature_text(content_encoding, &body_bytes)
}

fn form_signature_body(mut fields: Vec<(String, String)>) -> Result<String, String> {
    common::string_helper::sort_current_culture_by(&mut fields, |field| &field.0)?;
    let mut output = String::new();
    for (key, value) in fields {
        output.push_str(&key);
        output.push_str(&value);
    }
    Ok(output)
}

/// 解析 multipart 请求并按 NetEngine 规则拼接签名体
async fn build_multipart_signature_body(
    content_type: &str,
    body_stream: impl Stream<Item = Result<Bytes, std::io::Error>> + Send,
) -> Result<String, String> {
    let boundary = multer::parse_boundary(content_type)
        .map_err(|error| format!("multipart boundary 解析失败: {error}"))?;
    let mut multipart = Multipart::new(body_stream, boundary);
    let mut fields = Vec::new();
    let mut files = Vec::new();

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|error| format!("multipart 读取失败: {error}"))?
    {
        let Some(disposition) = crate::form_disposition::read(field.headers())? else {
            continue;
        };
        let name = disposition.name.clone().unwrap_or_default();

        if crate::form_disposition::file_name(&disposition).is_some() {
            let mut hasher = Sha256::new();
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|error| format!("multipart 文件读取失败: {error}"))?
            {
                hasher.update(&chunk);
            }
            files.push((name, digest_hex(&hasher.finalize())));
        } else {
            let value = field
                .text()
                .await
                .map_err(|error| format!("multipart 字段读取失败: {error}"))?;
            crate::request_parameters::append_joined(&mut fields, name, value);
        }
    }

    common::string_helper::sort_current_culture_by(&mut files, |file| &file.0)?;

    let mut output = form_signature_body(fields)?;
    for (name, hash) in files {
        output.push_str(&name);
        output.push_str(&hash);
    }

    Ok(output)
}

fn digest_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

/// 返回 401 未授权响应
fn unauthorized(message: &str) -> Response {
    json_error_response(StatusCode::UNAUTHORIZED, message)
}
