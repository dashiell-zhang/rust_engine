/// 应用层上下文定义
pub mod context;

/// 应用层 HTTP 客户端工厂
pub mod http_client;

/// 应用层鉴权模型
pub mod auth;

/// 应用层错误兼容导出
pub mod error;

/// 应用层缓存抽象
pub mod cache;

/// 应用层文件存储抽象
pub mod storage;

/// 应用层邮件抽象
pub mod email;

/// 应用层短信抽象
pub mod sms;

/// 应用层 LLM 抽象
pub mod llm;

/// 跨进程事件契约与发布入口
pub mod events;

/// 应用服务契约
pub mod services;
