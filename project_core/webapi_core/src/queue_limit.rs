use crate::routes::MethodRouter;
use axum::{
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
};
use common::crypto_helper::{StringEncoding, md5_hash_data};
use distributed_lock::{DistributedLock, LockGuard};
use std::{sync::Arc, time::Duration};
use tracing::warn;

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

    /// 从获取锁起计算的固定租约秒数；0 表示请求结束时释放
    pub expiry_seconds: u64,
}

impl QueueLimitOptions {
    /// 创建默认队列限制配置
    pub fn new(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            reject_when_full: false,
            use_authorization: false,
            use_parameter: false,
            expiry_seconds: 0,
        }
    }

    /// 创建带 Authorization 分桶的队列限制配置
    pub fn with_authorization(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            reject_when_full: false,
            use_authorization: true,
            use_parameter: false,
            expiry_seconds: 0,
        }
    }

    /// 创建立即拒绝超限请求的队列限制配置
    pub fn rejecting(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            reject_when_full: true,
            use_authorization: false,
            use_parameter: false,
            expiry_seconds: 0,
        }
    }

    /// 创建带 Authorization 分桶且立即拒绝超限请求的队列限制配置
    pub fn rejecting_with_authorization(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            reject_when_full: true,
            use_authorization: true,
            use_parameter: false,
            expiry_seconds: 0,
        }
    }

    /// 启用按参数分桶
    pub fn with_parameter(mut self) -> Self {
        self.use_parameter = true;
        self
    }

    /// 设置从获取锁开始的固定保留时间
    pub fn with_expiry_seconds(mut self, expiry_seconds: u64) -> Self {
        self.expiry_seconds = expiry_seconds;
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
    action: String,
}

impl QueueLimitState {
    /// 创建队列限制中间件状态
    pub fn new(
        distributed_lock: Arc<dyn DistributedLock>,
        options: QueueLimitOptions,
        action: impl Into<String>,
    ) -> Self {
        Self {
            distributed_lock,
            options,
            action: action.into(),
        }
    }
}

/// 为单个路由挂载队列限制中间件
pub fn queue_limit_method_router<TState>(
    route: MethodRouter<TState>,
    distributed_lock: Arc<dyn DistributedLock>,
    options: QueueLimitOptions,
    action: impl Into<String>,
) -> MethodRouter<TState>
where
    TState: Clone + Send + Sync + 'static,
{
    route.map_action(|route| {
        route.route_layer(from_fn_with_state(
            QueueLimitState::new(distributed_lock, options, action),
            queue_limit_middleware,
        ))
    })
}

/// 队列限制发生基础设施故障时继续执行；阻断模式返回业务 400。
pub async fn queue_limit_middleware(
    State(state): State<QueueLimitState>,
    request: Request,
    next: Next,
) -> Response {
    let mut seed = state.action.clone();
    if state.options.use_authorization {
        seed.push('_');
        seed.push_str(&common::string_helper::join_string_values(
            request
                .headers()
                .get_all(AUTHORIZATION)
                .iter()
                .filter_map(|v| v.to_str().ok()),
        ));
    }
    let request = if state.options.use_parameter {
        let (request, parameters) = match crate::request_parameters::read(request).await {
            Ok(result) => result,
            Err(response) => return *response,
        };
        let parameters = match parameters
            .and_then(|value| serde_json::to_string(&value).map_err(|e| e.to_string()))
        {
            Ok(value) => value,
            Err(error) => {
                warn!(target: "webapi.queue_limit", %error, "队列限制参数读取失败，已跳过限制");
                return next.run(request).await;
            }
        };
        seed.push('_');
        seed.push_str(&parameters);
        request
    } else {
        request
    };
    let key = format!("QueueLimit_{}", md5_hash_data(&seed, StringEncoding::Hex));
    let guard = match acquire_queue_limit_guard(&state, &key).await {
        Ok(Some(guard)) => guard,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                crate::extract::Json(crate::error::ErrBody {
                    err_msg: "请勿频繁操作".into(),
                }),
            )
                .into_response();
        }
        Err(error) => {
            warn!(target: "webapi.queue_limit", %key, %error, "队列限制获取锁失败，已跳过限制");
            return next.run(request).await;
        }
    };
    // 正数 Expiry 不随 Action 完成、取消或 panic 主动释放，也不续期。
    let guard = if state.options.expiry_seconds > 0 {
        guard.leave_until_expiry();
        None
    } else {
        Some(guard)
    };
    let response = next.run(request).await;
    if let Some(guard) = guard {
        guard.release().await;
    }
    response
}

async fn acquire_queue_limit_guard(
    state: &QueueLimitState,
    key: &str,
) -> Result<Option<LockGuard>, String> {
    let expiry = Duration::from_secs(if state.options.expiry_seconds > 0 {
        state.options.expiry_seconds
    } else {
        60
    });
    let semaphore = u32::try_from(state.options.max_concurrency.max(1))
        .map_err(|_| "并发数量超出支持范围".to_string())?;
    loop {
        match state
            .distributed_lock
            .try_lock(key, expiry, semaphore)
            .await
        {
            Ok(Some(guard)) => return Ok(Some(guard)),
            Ok(None) if state.options.reject_when_full => return Ok(None),
            Ok(None) => tokio::time::sleep(Duration::from_millis(200)).await,
            Err(error) => return Err(error.to_string()),
        }
    }
}
