/// 鉴权能力
pub mod auth;

/// 路由构建能力
pub mod builder;

/// body 缓冲上限能力
pub mod body_limit;

/// 请求参数提取能力
pub mod extract;

/// 响应缓存能力
pub mod cache_data;

/// 错误输出能力
pub mod error;

/// ETag 能力
pub mod etag;

/// 健康检查能力
pub mod health;

/// OpenAPI 公共能力
pub mod openapi;

/// WebAPI 基础 OpenAPI 文档
pub mod api_doc;

/// 请求日志能力
pub mod request_log;

/// 并发队列限制能力
pub mod queue_limit;

/// 限流能力
pub mod rate_limit;

/// 请求签名验证能力
pub mod sign_verify;

/// 宿主启动能力
pub mod startup;

/// WebAPI 宿主能力
pub mod host;

/// 兼容导出宿主配置模块
pub use host::settings;

/// 兼容导出宿主状态模块
pub use host::state;

/// 兼容导出宿主档案类型
pub use host::WebApiHostProfile;
