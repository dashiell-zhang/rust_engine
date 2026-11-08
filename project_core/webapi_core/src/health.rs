use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// 健康检查状态
pub const HEALTH_STATUS_HEALTHY: &str = "Healthy";

/// 不健康状态
pub const HEALTH_STATUS_UNHEALTHY: &str = "Unhealthy";

/// 单个组件健康检查结果
#[derive(Debug, Serialize)]
pub struct HealthComponentResponse {
    /// 组件名称
    pub name: String,

    /// 组件状态
    pub status: &'static str,

    /// 组件详情
    pub detail: String,
}

/// 内部健康检查报告；HTTP 端点只输出整体状态。
#[derive(Debug, Serialize)]
pub struct HealthzResponse {
    /// 服务名称
    pub service: String,

    /// 整体状态
    pub status: &'static str,

    /// 检查时间
    pub checked_at: String,

    /// 组件检查结果
    pub components: Vec<HealthComponentResponse>,
}

impl HealthzResponse {
    /// 判断聚合结果是否全部健康
    pub fn is_healthy(&self) -> bool {
        self.status == HEALTH_STATUS_HEALTHY
    }
}

/// 健康检查状态提供者
#[async_trait]
pub trait HealthState: Send + Sync {
    /// 生成包含组件明细的健康检查结果
    async fn health_report(&self) -> HealthzResponse;

    /// 支持取消的实现应等待后端清理后返回；旧实现保留原有调用方式。
    async fn health_report_with_cancellation(
        &self,
        _cancellation: &event_bus::CancellationToken,
    ) -> HealthzResponse {
        self.health_report().await
    }
}

/// 健康检查接口
pub async fn healthz<TState>(
    State(state): State<Arc<TState>>,
    cancellation: crate::request_cancellation::RequestCancellation,
) -> Response
where
    TState: HealthState + Send + Sync + 'static,
{
    let report = state.health_report_with_cancellation(&cancellation.0).await;

    // 根据聚合状态返回 HTTP 状态码
    let status_code = if report.is_healthy() {
        StatusCode::OK
    } else {
        tracing::error!(
            target: "app.health",
            service = %report.service,
            components = ?report.components,
            "健康检查失败"
        );
        StatusCode::SERVICE_UNAVAILABLE
    };

    let mut response = (status_code, report.status).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(
        header::EXPIRES,
        HeaderValue::from_static("Thu, 01 Jan 1970 00:00:00 GMT"),
    );
    response
}

/// 启动十秒后首次检查，之后每六十秒检查；停止时取消当前检查。
pub(crate) async fn publish_health<TState: HealthState>(
    state: Arc<TState>,
    cancellation: event_bus::CancellationToken,
) {
    use futures_util::FutureExt;
    use std::{panic::AssertUnwindSafe, time::Duration};
    let mut interval = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(10),
        Duration::from_secs(60),
    );
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => return,
            _ = interval.tick() => {},
        }
        let check_stopping = cancellation.child_token();
        let check =
            AssertUnwindSafe(state.health_report_with_cancellation(&check_stopping)).catch_unwind();
        tokio::pin!(check);
        let result = tokio::select! {
            _ = cancellation.cancelled() => {
                check_stopping.cancel();
                let _ = check.await;
                return;
            },
            result = &mut check => Some(result),
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                check_stopping.cancel();
                let _ = check.await;
                None
            },
        };
        match result {
            Some(Ok(report)) if !report.is_healthy() => {
                tracing::error!(target: "app.health", service = %report.service, components = ?report.components, "后台健康检查失败");
            }
            Some(Ok(_)) => {}
            Some(Err(_)) => tracing::error!(target: "app.health", "后台健康检查发生异常"),
            None => tracing::error!(target: "app.health", "后台健康检查超时"),
        }
    }
}
