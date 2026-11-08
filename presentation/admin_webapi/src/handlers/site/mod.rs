use application_model::site::models::{EditSiteDto, ServerInfoDto, SiteDto};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use webapi_core::extract::Json;
use webapi_core::{auth::UserContext, error::AppError};
use webapi_support::WebApiApplicationExt;

use crate::state::AppState;

/// 获取服务器基本信息（主机名、OS、CPU、内存、端口等）
#[utoipa::path(
    get,
    path = "/Site/GetServerInfo",
    responses(
        (status = 200, description = "查询成功", body = Vec<ServerInfoDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "站点管理"
)]
pub async fn get_server_info(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
) -> Response {
    // 内存读取失败时不影响接口成功 仅回退为 N A
    let memory_mb = get_process_memory_bytes()
        .map(|bytes| format!("{:.2}M", bytes as f64 / 1_048_576.0))
        .unwrap_or_else(|| "N/A".to_string());

    // 从配置中解析监听端口
    let listen_port = app_state
        .settings
        .host
        .listen_address
        .split(':')
        .next_back()
        .unwrap_or("N/A")
        .to_string();

    // 统一转换为名称和值的简单列表 方便前端直接渲染
    let items: Vec<ServerInfoDto> = vec![
        ServerInfoDto {
            name: "服务器名称".to_string(),
            value: get_hostname(),
        },
        ServerInfoDto {
            name: "操作系统".to_string(),
            value: std::env::consts::OS.to_string(),
        },
        ServerInfoDto {
            name: "监听地址".to_string(),
            value: app_state.settings.host.listen_address.clone(),
        },
        ServerInfoDto {
            name: "监听端口".to_string(),
            value: listen_port,
        },
        ServerInfoDto {
            name: "目录物理路径".to_string(),
            value: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        },
        ServerInfoDto {
            name: "服务器CPU".to_string(),
            value: format!("{}核", num_cpus()),
        },
        ServerInfoDto {
            name: "进程占用内存".to_string(),
            value: memory_mb,
        },
    ];

    (StatusCode::OK, Json(items)).into_response()
}

/// 获取当前主机名
fn get_hostname() -> String {
    // 直接复用系统 hostname 命令 避免引入额外依赖
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// 获取 CPU 核心数
fn num_cpus() -> usize {
    // 使用标准库提供的可用并行度 结果更贴近容器配额
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// 尝试读取进程内存占用字节数
///
/// Linux 通过 /proc/self/status 读取 VmRSS；其他平台返回 None
fn get_process_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        // 只读取进程常驻内存 避免把虚拟内存大小误报给前端
        let content = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in content.lines() {
            if line.starts_with("VmRSS:") {
                let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    None
}

/// 获取站点信息
#[utoipa::path(
    get,
    path = "/Site/GetSite",
    responses(
        (status = 200, description = "查询成功", body = SiteDto),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "站点管理"
)]
pub async fn get_site(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
) -> Result<Response, AppError> {
    // Handler 仅负责协议转换 业务逻辑委托给应用服务
    let service = app_state.services().site_webapi();

    match service.get_site().await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新站点信息
#[utoipa::path(
    post,
    path = "/Site/EditSite",
    request_body = EditSiteDto,
    responses(
        (status = 200, description = "更新成功", body = bool),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "站点管理"
)]
pub async fn update_site(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<EditSiteDto>,
) -> Result<Response, AppError> {
    // 当前用户信息用于写入更新审计字段
    let service = app_state.services().site_webapi();
    let current_user = user_ctx.current_user();

    match service.update_site(dto, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
