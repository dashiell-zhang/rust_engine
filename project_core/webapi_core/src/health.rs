use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// 健康检查状态
pub const HEALTH_STATUS_HEALTHY: &str = "healthy";

/// 不健康状态
pub const HEALTH_STATUS_UNHEALTHY: &str = "unhealthy";

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

/// 健康检查响应
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
}

/// 健康检查接口
pub async fn healthz<TState>(State(state): State<Arc<TState>>) -> Response
where
    TState: HealthState + Send + Sync + 'static,
{
    let report = state.health_report().await;

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

    (status_code, Json(report)).into_response()
}
