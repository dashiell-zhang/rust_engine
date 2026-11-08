mod action;
/// 鉴权能力
pub mod auth;
mod form_disposition;
mod json_body;
mod problem_details;

/// 路由构建能力
pub mod builder;
mod cors;
mod https_policy;
mod listener;
/// 业务端点注册与兼容路径匹配。
pub mod routes;
mod static_content_types;
mod static_files;

/// HTTPS 监听与证书配置。
pub mod tls;

/// 请求协议、地址与代理上下文。
pub mod request_context;

/// HTTP 请求生命周期取消。
pub mod request_cancellation;

/// body 缓冲上限能力
pub mod body_limit;

/// 请求参数提取能力
pub mod extract;

/// 异步 JSON 数组响应。
pub mod json_stream;

mod object_response;
mod replay_body;
mod request_binding;
mod request_body;
mod request_parameters;
mod request_path;
mod response_charset;
mod response_compression;
mod response_format;
/// RSA 请求字段解密。
pub mod rsa_decrypt;
/// 控制器字符串结果。
pub mod text_response;

/// 响应缓存能力
pub mod cache_data;

/// 错误输出能力
pub mod error;

/// 宿主运行环境
pub mod environment;

/// ETag 能力
pub mod etag;

/// 健康检查能力
pub mod health;

/// 物理文件与图片下载响应。
pub mod file_response;

/// 上传文件的数据转换入口。
pub mod uploaded_file;

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
