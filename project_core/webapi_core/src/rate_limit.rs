use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{Request, State},
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use serde::Deserialize;

use crate::error::json_error_response;

/// 限流配置
#[derive(Deserialize, Clone)]
pub struct RateLimitSettings {
    /// 是否启用限流
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,

    /// 滑动窗口内最大请求数
    #[serde(default = "default_rate_limit_max_requests")]
    pub max_requests: u64,

    /// 滑动窗口大小（秒）
    #[serde(default = "default_rate_limit_window_seconds")]
    pub window_seconds: u64,
}

impl Default for RateLimitSettings {
    /// 创建默认限流配置
    fn default() -> Self {
        Self {
            enabled: default_rate_limit_enabled(),
            max_requests: default_rate_limit_max_requests(),
            window_seconds: default_rate_limit_window_seconds(),
        }
    }
}

/// 进程内滑动窗口限流器
///
/// 使用滑动窗口日志算法，按客户端 IP 独立计数
/// 限流状态仅保留在当前进程内存中，多实例部署时每个实例独立计数
pub struct RateLimiter {
    /// 每个 IP 对应的请求时间戳队列
    counters: DashMap<String, VecDeque<Instant>>,

    /// 时间窗口内允许的最大请求数
    max_requests: usize,

    /// 滑动窗口时长
    window: Duration,
}

impl RateLimiter {
    /// 创建限流器
    pub fn new(max_requests: u64, window_seconds: u64) -> Self {
        Self {
            counters: DashMap::new(),
            max_requests: max_requests as usize,
            window: Duration::from_secs(window_seconds),
        }
    }

    /// 检查指定键是否允许本次请求，允许时同时记录本次时间戳
    ///
    /// 返回 `true` 表示未超限，`false` 表示已超限
    pub fn check_and_record(&self, key: &str) -> bool {
        let now = Instant::now();
        let window = self.window;
        let max_requests = self.max_requests;

        let mut entry = self.counters.entry(key.to_string()).or_default();

        // 清除窗口外的过期记录
        while entry
            .front()
            .map(|t| now.duration_since(*t) > window)
            .unwrap_or(false)
        {
            entry.pop_front();
        }

        if entry.len() >= max_requests {
            return false;
        }

        entry.push_back(now);
        true
    }

    /// 获取窗口秒数，用于写入 Retry-After 响应头
    pub fn window_seconds(&self) -> u64 {
        self.window.as_secs()
    }
}

/// 限流中间件
///
/// 使用共享请求上下文中的客户端 IP 作为限流键
/// 超限时返回 HTTP 429 并携带 `Retry-After` 响应头
pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    context: crate::request_context::RequestContext,
    request: Request,
    next: Next,
) -> Response {
    let client_key = match context.client_ip() {
        Ok(ip) => ip,
        Err(error) => return crate::error::internal_error_response(error.message()),
    };

    if !limiter.check_and_record(client_key) {
        let retry_after = limiter.window_seconds().to_string();

        let mut response =
            json_error_response(StatusCode::TOO_MANY_REQUESTS, "请求过于频繁，请稍后再试");

        if let Ok(value) = HeaderValue::from_str(&retry_after) {
            response
                .headers_mut()
                .insert(HeaderName::from_static("retry-after"), value);
        }

        return response;
    }

    next.run(request).await
}

/// 获取默认限流启用状态
fn default_rate_limit_enabled() -> bool {
    false
}

/// 获取默认最大请求数
fn default_rate_limit_max_requests() -> u64 {
    100
}

/// 获取默认滑动窗口秒数
fn default_rate_limit_window_seconds() -> u64 {
    60
}
