use crate::routes::MethodRouter;
use crate::{
    host::state::AppState, object_response::ObjectResponse, request_parameters::Parameters,
};
use axum::{
    body::Bytes,
    extract::{FromRequestParts, OriginalUri, RawPathParams, Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
};
use common::crypto_helper::{StringEncoding, md5_hash_data};
use distributed_cache::{CacheEntryOptions, DistributedCache};
use distributed_lock::{DistributedLock, LockGuard};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tracing::warn;

const CACHE_LOCK_EXPIRY_SECONDS: u64 = 60;
const CACHE_LOCK_RETRY_MILLIS: u64 = 200;

#[derive(Clone)]
pub struct CacheDataState {
    cache: Arc<dyn DistributedCache>,
    distributed_lock: Arc<dyn DistributedLock>,
    options: CacheDataOptions,
    action: String,
}

impl CacheDataState {
    pub fn new(
        cache: Arc<dyn DistributedCache>,
        distributed_lock: Arc<dyn DistributedLock>,
        options: CacheDataOptions,
        action: impl Into<String>,
    ) -> Self {
        Self {
            cache,
            distributed_lock,
            options,
            action: action.into(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct CacheDataOptions {
    pub ttl_seconds: u64,
    pub use_authorization: bool,
}

impl CacheDataOptions {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            ttl_seconds,
            use_authorization: false,
        }
    }
    pub fn with_authorization(ttl_seconds: u64) -> Self {
        Self {
            ttl_seconds,
            use_authorization: true,
        }
    }
}

pub fn cache_method_router(
    route: MethodRouter<Arc<AppState>>,
    app_state: Arc<AppState>,
    ttl_seconds: u64,
    action: impl Into<String>,
) -> MethodRouter<Arc<AppState>> {
    cache_method_router_with_options(route, app_state, CacheDataOptions::new(ttl_seconds), action)
}

/// action 使用包含宿主及模块的处理器身份，与权限模块配置和请求路径分开。
pub fn cache_method_router_with_options(
    route: MethodRouter<Arc<AppState>>,
    app_state: Arc<AppState>,
    options: CacheDataOptions,
    action: impl Into<String>,
) -> MethodRouter<Arc<AppState>> {
    route.map_action(|route| {
        route.route_layer(from_fn_with_state(
            CacheDataState::new(
                app_state.cache.clone(),
                app_state.distributed_lock.clone(),
                options,
                action,
            ),
            cache_data_middleware,
        ))
    })
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedResponse {
    status_code: u16,
    value: Box<RawValue>,
}

impl CachedResponse {
    fn into_response(self) -> Option<Response> {
        let status = StatusCode::from_u16(self.status_code).ok()?;
        let mut response = if self.value.get().starts_with('"') {
            // 对齐 CachedResponse.ToObjectResult：字符串 JsonElement 恢复为字符串结果。
            let text = serde_json::from_str::<String>(self.value.get()).ok()?;
            crate::text_response::Text(text).into_response()
        } else {
            // 源 JsonElement 写回仍经过编码器；保留数字词法、重复键和原字段顺序。
            let json: Bytes = common::json_helper::object_to_json(&self.value)
                .ok()?
                .into();
            ObjectResponse {
                internal_json: Some(json.clone()),
                json,
            }
            .into_response()
        };
        *response.status_mut() = status;
        Some(response)
    }
}

enum CacheAcquireResult {
    Hit(Response),
    Locked(LockGuard),
    Bypass,
}

/// 对象缓存只保存返回值和状态；不缓存 null、文件、流和异常处理结果。
pub async fn cache_data_middleware(
    State(state): State<CacheDataState>,
    request: Request,
    next: Next,
) -> Response {
    let (request, key) = match build_cache_request_and_key(request, &state).await {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let Some(key) = key else {
        return next.run(request).await;
    };
    let guard = match acquire_cache_guard(&state, &key).await {
        CacheAcquireResult::Hit(response) => return response,
        CacheAcquireResult::Locked(guard) => Some(guard),
        CacheAcquireResult::Bypass => None,
    };
    let response = crate::object_response::CACHE_SERIALIZATION
        .scope(state.options.ttl_seconds > 0, next.run(request))
        .await;
    let object = response.extensions().get::<ObjectResponse>().cloned();
    if let Some(object) = object.filter(|object| !object.is_null())
        && let Some(cache_json) = object.internal_json
        && state.options.ttl_seconds > 0
    {
        let payload = std::str::from_utf8(&cache_json)
            .ok()
            .and_then(|json| RawValue::from_string(json.to_string()).ok())
            .and_then(|value| {
                serde_json::to_vec(&CachedResponse {
                    status_code: response.status().as_u16(),
                    value,
                })
                .ok()
            });
        if let Some(payload) = payload
            && let Err(error) = state
                .cache
                .set_bytes(
                    &key,
                    payload,
                    CacheEntryOptions::relative(Duration::from_secs(state.options.ttl_seconds)),
                )
                .await
        {
            warn!(target: "webapi.cache_data", cache_key = %key, %error, "写入响应缓存失败");
        }
    }
    if let Some(guard) = guard {
        guard.release().await;
    }
    response
}

async fn acquire_cache_guard(state: &CacheDataState, key: &str) -> CacheAcquireResult {
    loop {
        match state.cache.get_bytes(key).await {
            Ok(Some(payload)) => match serde_json::from_slice::<CachedResponse>(&payload)
                .ok()
                .and_then(CachedResponse::into_response)
            {
                Some(response) => return CacheAcquireResult::Hit(response),
                None => {
                    warn!(target: "webapi.cache_data", cache_key = %key, "响应缓存内容无效，已回源");
                    return CacheAcquireResult::Bypass;
                }
            },
            Ok(None) => {}
            Err(error) => {
                warn!(target: "webapi.cache_data", cache_key = %key, %error, "读取响应缓存失败，已回源");
                return CacheAcquireResult::Bypass;
            }
        }
        match state
            .distributed_lock
            .try_lock(key, Duration::from_secs(CACHE_LOCK_EXPIRY_SECONDS), 1)
            .await
        {
            Ok(Some(guard)) => return CacheAcquireResult::Locked(guard),
            Ok(None) => tokio::time::sleep(Duration::from_millis(CACHE_LOCK_RETRY_MILLIS)).await,
            Err(error) => {
                warn!(target: "webapi.cache_data", cache_key = %key, %error, "获取响应缓存锁失败，已回源");
                return CacheAcquireResult::Bypass;
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheKeyData {
    action: String,
    method: String,
    path_base: String,
    path: String,
    route_values: BTreeMap<String, String>,
    parameters: Parameters,
    authorization: Option<String>,
}

async fn build_cache_request_and_key(
    request: Request,
    state: &CacheDataState,
) -> Result<(Request, Option<String>), Box<Response>> {
    let (request, parameters) = crate::request_parameters::read(request).await?;
    let parameters = match parameters {
        Ok(parameters) => parameters,
        Err(error) => {
            warn!(target: "webapi.cache_data", %error, "请求缓存参数读取失败，已回源");
            return Ok((request, None));
        }
    };
    let (mut parts, body) = request.into_parts();
    let route_values = RawPathParams::from_request_parts(&mut parts, &())
        .await
        .ok()
        .map(|values| {
            values
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let path = crate::request_path::value(&parts.uri, &parts.extensions).to_string();
    let original_path = parts
        .extensions
        .get::<OriginalUri>()
        .map(|uri| uri.0.path())
        .unwrap_or(&path);
    let key_data = CacheKeyData {
        action: state.action.clone(),
        method: parts.method.to_string(),
        path_base: original_path
            .strip_suffix(&path)
            .unwrap_or_default()
            .to_string(),
        path,
        route_values,
        parameters,
        authorization: state.options.use_authorization.then(|| {
            common::string_helper::join_string_values(
                parts
                    .headers
                    .get_all(AUTHORIZATION)
                    .iter()
                    .filter_map(|value| value.to_str().ok()),
            )
        }),
    };
    let request = Request::from_parts(parts, body);
    let key = serde_json::to_string(&key_data)
        .ok()
        .map(|seed| format!("CacheData_{}", md5_hash_data(&seed, StringEncoding::Hex)));
    Ok((request, key))
}
