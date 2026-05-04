use std::time::Duration;

use axum::{
    Json,
    body::{Body, Bytes, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::MethodRouter,
};
use distributed_lock::{DistributedLock, LockGuard};
use sha2::{Digest, Sha256};
use std::{mem::ManuallyDrop, sync::Arc};
use tracing::warn;

use crate::body_limit::{DEFAULT_BUFFERED_BODY_LIMIT_BYTES, content_length_exceeds};
use crate::error::ErrBody;

/// 队列限制配置
#[derive(Clone, Copy)]
pub struct QueueLimitOptions {
    /// 单个键允许的最大并发数
    pub max_concurrency: usize,

    /// 超限时是否立即拒绝
    pub reject_when_full: bool,

    /// 是否将 Authorization 请求头纳入限制键
    pub use_authorization: bool,

    /// 是否将请求参数纳入限制键
    pub use_parameter: bool,

    /// 请求完成后继续持有并发槽位的时长，单位秒
    pub hold_seconds: u64,
}

impl QueueLimitOptions {
    /// 创建默认队列限制配置
    pub fn new(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            reject_when_full: false,
            use_authorization: false,
            use_parameter: false,
            hold_seconds: 0,
        }
    }

    /// 创建带 Authorization 分桶的队列限制配置
    pub fn with_authorization(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            reject_when_full: false,
            use_authorization: true,
            use_parameter: false,
            hold_seconds: 0,
        }
    }

    /// 创建立即拒绝超限请求的队列限制配置
    pub fn rejecting(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            reject_when_full: true,
            use_authorization: false,
            use_parameter: false,
            hold_seconds: 0,
        }
    }

    /// 创建带 Authorization 分桶且立即拒绝超限请求的队列限制配置
    pub fn rejecting_with_authorization(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            reject_when_full: true,
            use_authorization: true,
            use_parameter: false,
            hold_seconds: 0,
        }
    }

    /// 启用按参数分桶
    pub fn with_parameter(mut self) -> Self {
        self.use_parameter = true;
        self
    }

    /// 设置请求完成后的持有时长
    pub fn with_hold_seconds(mut self, hold_seconds: u64) -> Self {
        self.hold_seconds = hold_seconds;
        self
    }
}

/// 队列限制中间件状态
#[derive(Clone)]
pub struct QueueLimitState {
    /// 队列限制依赖的分布式锁
    distributed_lock: Arc<dyn DistributedLock>,

    /// 当前路由使用的限制配置
    options: QueueLimitOptions,
}

impl QueueLimitState {
    /// 创建队列限制中间件状态
    pub fn new(distributed_lock: Arc<dyn DistributedLock>, options: QueueLimitOptions) -> Self {
        Self {
            distributed_lock,
            options,
        }
    }
}

/// 为单个路由挂载队列限制中间件
pub fn queue_limit_method_router<TState>(
    route: MethodRouter<TState>,
    distributed_lock: Arc<dyn DistributedLock>,
    options: QueueLimitOptions,
) -> MethodRouter<TState>
where
    TState: Clone + Send + Sync + 'static,
{
    route.layer(from_fn_with_state(
        QueueLimitState::new(distributed_lock, options),
        queue_limit_middleware,
    ))
}

/// 队列限制中间件
pub async fn queue_limit_middleware(
    State(state): State<QueueLimitState>,
    request: Request,
    next: Next,
) -> Response {
    let (request, key) = match build_queue_limit_request_and_key(request, state.options).await {
        Ok(value) => value,
        Err(response) => return response,
    };

    let lock_guard = match acquire_queue_limit_guard(&state, &key).await {
        Ok(Some(guard)) => guard,
        Ok(None) => return too_many_requests_response(),
        Err(error) => {
            warn!(target: "webapi.queue_limit", key = %key, error = %error, "队列限制获取锁失败，已跳过限制");
            return next.run(request).await;
        }
    };

    let response = next.run(request).await;

    if state.options.hold_seconds > 0 {
        let hold_seconds = state.options.hold_seconds;
        tokio::spawn(async move {
            let guard = ManuallyDrop::new(lock_guard);
            tokio::time::sleep(Duration::from_secs(hold_seconds)).await;
            let guard = ManuallyDrop::into_inner(guard);
            drop(guard);
        });
    } else {
        drop(lock_guard);
    }

    response
}

/// 获取队列限制锁
async fn acquire_queue_limit_guard(
    state: &QueueLimitState,
    key: &str,
) -> Result<Option<LockGuard>, String> {
    let expiry = queue_limit_expiry(state.options);
    let semaphore = normalize_max_concurrency(state.options);

    if state.options.reject_when_full {
        return state
            .distributed_lock
            .try_lock(key, expiry, semaphore)
            .await
            .map_err(|error| error.to_string());
    }

    loop {
        match state
            .distributed_lock
            .try_lock(key, expiry, semaphore)
            .await
        {
            Ok(Some(guard)) => return Ok(Some(guard)),
            Ok(None) => tokio::time::sleep(Duration::from_millis(200)).await,
            Err(error) => return Err(error.to_string()),
        }
    }
}

/// 生成限制键并在必要时重建请求体
async fn build_queue_limit_request_and_key(
    request: Request,
    options: QueueLimitOptions,
) -> Result<(Request, String), Response> {
    if !options.use_parameter {
        let key = build_queue_limit_key(
            request.method(),
            request.uri().path(),
            request.uri().query(),
            request.headers(),
            None,
            options,
        );

        return Ok((request, key));
    }

    let (parts, body) = request.into_parts();
    if content_length_exceeds(&parts.headers, DEFAULT_BUFFERED_BODY_LIMIT_BYTES) {
        return Err(payload_too_large_response());
    }

    let body_bytes = match to_bytes(body, DEFAULT_BUFFERED_BODY_LIMIT_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return Err(payload_too_large_response()),
    };

    let key = build_queue_limit_key(
        &parts.method,
        parts.uri.path(),
        parts.uri.query(),
        &parts.headers,
        Some(&body_bytes),
        options,
    );

    Ok((Request::from_parts(parts, Body::from(body_bytes)), key))
}

/// 构建队列限制键
fn build_queue_limit_key(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: Option<&Bytes>,
    options: QueueLimitOptions,
) -> String {
    let mut seed = format!("{}:{}", method, path);

    if options.use_parameter {
        let normalized_query = normalize_query(query);
        if !normalized_query.is_empty() {
            seed.push('_');
            seed.push_str(&normalized_query);
        }

        if let Some(body) = body
            && !body.is_empty()
        {
            seed.push('_');
            seed.push_str(&sha256_hex_bytes(body));
        }
    }

    if options.use_authorization
        && let Some(authorization) = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
        && !authorization.is_empty()
    {
        seed.push('_');
        seed.push_str(authorization);
    }

    format!("QueueLimit_{}", sha256_hex(&seed))
}

/// 规范化查询串
fn normalize_query(query: Option<&str>) -> String {
    let Some(query) = query else {
        return String::new();
    };

    let mut pairs = url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();

    pairs.sort();

    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(&key, &value);
    }

    serializer.finish()
}

/// 归一化最大并发数
fn normalize_max_concurrency(options: QueueLimitOptions) -> u32 {
    options.max_concurrency.max(1) as u32
}

/// 获取锁的失效时长
fn queue_limit_expiry(options: QueueLimitOptions) -> Duration {
    if options.hold_seconds > 0 {
        return Duration::from_secs(options.hold_seconds);
    }

    Duration::from_secs(60)
}

/// 生成 429 响应
fn too_many_requests_response() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ErrBody {
            err_msg: "请勿频繁操作".to_string(),
        }),
    )
        .into_response()
}

/// 生成请求体过大响应
fn payload_too_large_response() -> Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(ErrBody {
            err_msg: "请求体超过允许大小".to_string(),
        }),
    )
        .into_response()
}

/// 计算 SHA-256 十六进制摘要
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();

    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 计算字节数组 SHA-256 十六进制摘要
fn sha256_hex_bytes(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let hash = hasher.finalize();

    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}
