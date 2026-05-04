use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    Json,
    body::{Body, Bytes, to_bytes},
    extract::{Request, State},
    http::{Method, StatusCode, header::CONTENT_TYPE},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::MethodRouter,
};
use common::crypto_helper::{StringEncoding, md5_hash_data};
use distributed_cache::{CacheEntryOptions, DistributedCache};
use distributed_lock::{DistributedLock, LockGuard};
use futures_util::stream;
use multer::Multipart;
use serde_json::to_string as to_json_string;
use tracing::warn;
use url::form_urlencoded;

use crate::body_limit::{
    DEFAULT_BUFFERED_BODY_LIMIT_BYTES, content_length_exceeds, has_content_length,
};
use crate::error::ErrBody;
use crate::host::state::AppState;

const CACHE_LOCK_EXPIRY_SECONDS: u64 = 60;
const CACHE_LOCK_RETRY_MILLIS: u64 = 200;
const CACHED_RESPONSE_VERSION: u8 = 1;

/// 响应缓存中间件状态
#[derive(Clone)]
pub struct CacheDataState {
    /// 缓存接口
    cache: Arc<dyn DistributedCache>,

    /// 分布式锁接口
    distributed_lock: Arc<dyn DistributedLock>,

    /// 响应缓存配置
    options: CacheDataOptions,
}

impl CacheDataState {
    /// 创建响应缓存中间件状态
    pub fn new(
        cache: Arc<dyn DistributedCache>,
        distributed_lock: Arc<dyn DistributedLock>,
        options: CacheDataOptions,
    ) -> Self {
        Self {
            cache,
            distributed_lock,
            options,
        }
    }
}

/// 响应缓存配置
#[derive(Clone, Copy)]
pub struct CacheDataOptions {
    /// 缓存时长，单位秒
    pub ttl_seconds: u64,

    /// 是否将 Authorization 请求头纳入缓存键
    pub use_authorization: bool,
}

impl CacheDataOptions {
    /// 创建仅包含 TTL 的缓存配置
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            ttl_seconds,
            use_authorization: false,
        }
    }

    /// 创建包含 Authorization 分桶的缓存配置
    pub fn with_authorization(ttl_seconds: u64) -> Self {
        Self {
            ttl_seconds,
            use_authorization: true,
        }
    }
}

/// 为单个路由挂载响应缓存中间件
pub fn cache_method_router(
    route: MethodRouter<Arc<AppState>>,
    app_state: Arc<AppState>,
    ttl_seconds: u64,
) -> MethodRouter<Arc<AppState>> {
    cache_method_router_with_options(route, app_state, CacheDataOptions::new(ttl_seconds))
}

/// 为单个路由挂载带配置的响应缓存中间件
pub fn cache_method_router_with_options(
    route: MethodRouter<Arc<AppState>>,
    app_state: Arc<AppState>,
    options: CacheDataOptions,
) -> MethodRouter<Arc<AppState>> {
    route.layer(from_fn_with_state(
        CacheDataState::new(
            app_state.cache.clone(),
            app_state.role_code_lock.clone(),
            options,
        ),
        cache_data_middleware,
    ))
}

enum CacheLookupResult {
    Hit(Response),
    Miss,
}

enum CacheAcquireResult {
    Hit(Response),
    Locked(LockGuard),
    Bypass,
}

enum CacheRequestBuildResult {
    Cache(Request, String),
    Bypass(Request),
}

/// 响应缓存中间件
pub async fn cache_data_middleware(
    State(cache_state): State<CacheDataState>,
    request: Request,
    next: Next,
) -> Response {
    if cache_state.options.ttl_seconds == 0 {
        return next.run(request).await;
    }

    let (request, cache_key) = match build_cache_request_and_key(request, cache_state.options).await
    {
        Ok(CacheRequestBuildResult::Cache(request, cache_key)) => (request, cache_key),
        Ok(CacheRequestBuildResult::Bypass(request)) => return next.run(request).await,
        Err(response) => return response,
    };

    let lock_guard = match acquire_cache_guard(&cache_state, &cache_key).await {
        CacheAcquireResult::Hit(response) => return response,
        CacheAcquireResult::Locked(guard) => Some(guard),
        CacheAcquireResult::Bypass => None,
    };

    let response = next.run(request).await;

    if response.status() != StatusCode::OK {
        return response;
    }

    if !has_content_length(response.headers())
        || content_length_exceeds(response.headers(), DEFAULT_BUFFERED_BODY_LIMIT_BYTES)
    {
        drop(lock_guard);
        return response;
    }

    let (parts, body) = response.into_parts();
    let bytes = match to_bytes(body, DEFAULT_BUFFERED_BODY_LIMIT_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => {
            warn!(target: "webapi.cache_data", cache_key = %cache_key, error = %error, "读取响应体失败，无法继续输出响应");
            drop(lock_guard);
            return invalid_response_body_response();
        }
    };

    if bytes.is_empty() {
        drop(lock_guard);
        return Response::from_parts(parts, Body::from(bytes));
    }

    let content_type = parts
        .headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());

    match build_cached_payload(content_type, &bytes) {
        Ok(payload) => {
            if let Err(error) = cache_state
                .cache
                .set_bytes(
                    &cache_key,
                    payload,
                    CacheEntryOptions::relative(std::time::Duration::from_secs(
                        cache_state.options.ttl_seconds,
                    )),
                )
                .await
            {
                warn!(target: "webapi.cache_data", cache_key = %cache_key, error = %error, "写入响应缓存失败");
            }
        }
        Err(error) => {
            warn!(target: "webapi.cache_data", cache_key = %cache_key, error = %error, "组装响应缓存失败");
        }
    }

    drop(lock_guard);
    Response::from_parts(parts, Body::from(bytes))
}

async fn build_cache_request_and_key(
    request: Request,
    options: CacheDataOptions,
) -> Result<CacheRequestBuildResult, Response> {
    if request.method() == Method::GET || request.method() == Method::HEAD {
        let cache_key = build_cache_key_from_seed(
            serialize_parameters(&collect_query_parameters(request.uri().query())),
            request_authorization(&request),
            options,
        );
        return Ok(CacheRequestBuildResult::Cache(request, cache_key));
    }

    if !has_content_length(request.headers())
        || content_length_exceeds(request.headers(), DEFAULT_BUFFERED_BODY_LIMIT_BYTES)
    {
        return Ok(CacheRequestBuildResult::Bypass(request));
    }

    let (parts, body) = request.into_parts();
    let query = parts.uri.query().map(ToOwned::to_owned);
    let content_type = parts
        .headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let authorization = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let body_bytes = match to_bytes(body, DEFAULT_BUFFERED_BODY_LIMIT_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => {
            warn!(target: "webapi.cache_data", method = %parts.method, path = %parts.uri.path(), error = %error, "读取请求体失败，无法继续处理请求");
            return Err(payload_too_large_response());
        }
    };

    let seed =
        build_parameter_seed(query.as_deref(), content_type.as_deref(), Some(&body_bytes)).await;
    let request = Request::from_parts(parts, Body::from(body_bytes.clone()));
    let cache_key = build_cache_key_from_seed(seed, authorization.as_deref(), options);

    Ok(CacheRequestBuildResult::Cache(request, cache_key))
}

async fn acquire_cache_guard(state: &CacheDataState, cache_key: &str) -> CacheAcquireResult {
    loop {
        match lookup_cached_response(state, cache_key).await {
            CacheLookupResult::Hit(response) => return CacheAcquireResult::Hit(response),
            CacheLookupResult::Miss => {}
        }

        match state
            .distributed_lock
            .try_lock(cache_key, Duration::from_secs(CACHE_LOCK_EXPIRY_SECONDS), 1)
            .await
        {
            Ok(Some(guard)) => return CacheAcquireResult::Locked(guard),
            Ok(None) => tokio::time::sleep(Duration::from_millis(CACHE_LOCK_RETRY_MILLIS)).await,
            Err(error) => {
                warn!(target: "webapi.cache_data", cache_key = %cache_key, error = %error, "获取响应缓存锁失败，已回源");
                return CacheAcquireResult::Bypass;
            }
        }
    }
}

async fn lookup_cached_response(state: &CacheDataState, cache_key: &str) -> CacheLookupResult {
    match state.cache.get_bytes(cache_key).await {
        Ok(Some(payload)) => match try_build_cached_response(&payload) {
            Some(response) => CacheLookupResult::Hit(response),
            None => {
                warn!(target: "webapi.cache_data", cache_key = %cache_key, "响应缓存命中但缓存内容无效，已回源");

                if let Err(error) = state.cache.remove(cache_key).await {
                    warn!(target: "webapi.cache_data", cache_key = %cache_key, error = %error, "清理无效响应缓存失败");
                }

                CacheLookupResult::Miss
            }
        },
        Ok(None) => CacheLookupResult::Miss,
        Err(error) => {
            warn!(target: "webapi.cache_data", cache_key = %cache_key, error = %error, "读取响应缓存失败，已回源");
            CacheLookupResult::Miss
        }
    }
}

/// 按 .NET 规则构建参数种子
async fn build_parameter_seed(
    query: Option<&str>,
    content_type: Option<&str>,
    body: Option<&Bytes>,
) -> String {
    let mut parameters = collect_query_parameters(query);

    if let Some(body) = body
        && !body.is_empty()
    {
        append_body_parameters(&mut parameters, content_type, body).await;
    }

    serialize_parameters(&parameters)
}

fn build_cache_key_from_seed(
    mut seed: String,
    authorization: Option<&str>,
    options: CacheDataOptions,
) -> String {
    if options.use_authorization
        && let Some(authorization) = authorization
        && !authorization.is_empty()
    {
        if !seed.is_empty() {
            seed.push('_');
        }
        seed.push_str(authorization);
    }

    format!("CacheData_{}", md5_hash_data(&seed, StringEncoding::Hex))
}

fn request_authorization(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
}

fn payload_too_large_response() -> Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(ErrBody {
            err_msg: "请求体超过允许大小".to_string(),
        }),
    )
        .into_response()
}

fn invalid_response_body_response() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrBody {
            err_msg: "响应体读取失败".to_string(),
        }),
    )
        .into_response()
}

fn collect_query_parameters(query: Option<&str>) -> Vec<(String, String)> {
    let Some(query) = query else {
        return Vec::new();
    };

    collect_joined_pairs(form_urlencoded::parse(query.as_bytes()))
}

async fn append_body_parameters(
    parameters: &mut Vec<(String, String)>,
    content_type: Option<&str>,
    body: &Bytes,
) {
    let Some(content_type) = content_type else {
        append_raw_body_parameter(parameters, body);
        return;
    };

    if content_type.starts_with("application/x-www-form-urlencoded") {
        let mut fields = collect_joined_pairs(form_urlencoded::parse(body.as_ref()));
        fields.sort_by(|left, right| left.0.cmp(&right.0));
        parameters.extend(fields);
        return;
    }

    if content_type.starts_with("multipart/form-data") {
        match collect_multipart_parameters(content_type, body).await {
            Ok(mut fields) => {
                fields.sort_by(|left, right| left.0.cmp(&right.0));
                parameters.extend(fields);
            }
            Err(error) => {
                warn!(target: "webapi.cache_data", error = %error, "解析 multipart 请求参数失败，已回退到原始 body 建键");
                append_raw_body_parameter(parameters, body);
            }
        }

        return;
    }

    append_raw_body_parameter(parameters, body);
}

fn collect_joined_pairs<'a, I>(pairs: I) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (std::borrow::Cow<'a, str>, std::borrow::Cow<'a, str>)>,
{
    let mut output = Vec::<(String, String)>::new();

    for (key, value) in pairs {
        let key = key.into_owned();
        let value = value.into_owned();

        if let Some((_, existing_value)) = output
            .iter_mut()
            .find(|(existing_key, _)| *existing_key == key)
        {
            if !existing_value.is_empty() {
                existing_value.push(',');
            }
            existing_value.push_str(&value);
        } else {
            output.push((key, value));
        }
    }

    output
}

async fn collect_multipart_parameters(
    content_type: &str,
    body: &Bytes,
) -> Result<Vec<(String, String)>, String> {
    let boundary = multer::parse_boundary(content_type)
        .map_err(|error| format!("multipart boundary 解析失败: {error}"))?;
    let body_stream = stream::once(async move { Ok::<Bytes, Infallible>(body.clone()) });
    let mut multipart = Multipart::new(body_stream, boundary);
    let mut fields = Vec::<(String, String)>::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| format!("multipart 读取失败: {error}"))?
    {
        if field.file_name().is_some() {
            continue;
        }

        let name = field.name().unwrap_or_default().to_string();
        let value = field
            .text()
            .await
            .map_err(|error| format!("multipart 字段读取失败: {error}"))?;

        if let Some((_, existing_value)) = fields
            .iter_mut()
            .find(|(existing_key, _)| *existing_key == name)
        {
            if !existing_value.is_empty() {
                existing_value.push(',');
            }
            existing_value.push_str(&value);
        } else {
            fields.push((name, value));
        }
    }

    Ok(fields)
}

fn append_raw_body_parameter(parameters: &mut Vec<(String, String)>, body: &Bytes) {
    let body_text = String::from_utf8_lossy(body).into_owned();

    if !body_text.is_empty() {
        parameters.push(("body".to_string(), body_text));
    }
}

fn serialize_parameters(parameters: &[(String, String)]) -> String {
    let mut output = String::from("{");

    for (index, (key, value)) in parameters.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }

        let Ok(encoded_key) = to_json_string(key) else {
            continue;
        };
        let Ok(encoded_value) = to_json_string(value) else {
            continue;
        };

        output.push_str(&encoded_key);
        output.push(':');
        output.push_str(&encoded_value);
    }

    output.push('}');
    output
}

/// 组装缓存载荷
fn build_cached_payload(content_type: Option<&str>, body: &[u8]) -> Result<Vec<u8>, String> {
    let content_type_bytes = content_type.unwrap_or_default().as_bytes();
    let content_type_len =
        u32::try_from(content_type_bytes.len()).map_err(|_| "响应内容类型过长".to_string())?;
    let mut payload = Vec::with_capacity(1 + 1 + 4 + content_type_bytes.len() + body.len());

    payload.push(CACHED_RESPONSE_VERSION);
    payload.push(u8::from(content_type.is_some()));
    payload.extend_from_slice(&content_type_len.to_le_bytes());
    payload.extend_from_slice(content_type_bytes);
    payload.extend_from_slice(body);

    Ok(payload)
}

/// 将缓存内容还原为 HTTP 响应
fn try_build_cached_response(payload: &[u8]) -> Option<Response> {
    if payload.len() < 6 || payload[0] != CACHED_RESPONSE_VERSION {
        return None;
    }

    let has_content_type = payload[1] != 0;
    let content_type_len = u32::from_le_bytes(payload[2..6].try_into().ok()?) as usize;
    let body_offset = 6 + content_type_len;

    if payload.len() < body_offset {
        return None;
    }

    let content_type = if has_content_type {
        Some(std::str::from_utf8(&payload[6..body_offset]).ok()?)
    } else {
        None
    };

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(payload[body_offset..].to_vec()))
        .ok()?;

    if let Some(content_type) = content_type
        && let Ok(value) = content_type.parse()
    {
        response.headers_mut().insert(CONTENT_TYPE, value);
    }

    Some(response)
}
