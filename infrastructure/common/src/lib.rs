/// 任务配置和调度共用的 Cron 解析能力
#[cfg(feature = "runtime")]
pub mod cron_helper;

/// 加解密与摘要能力
#[cfg(feature = "runtime")]
pub mod crypto_helper;

/// 统一业务异常
pub mod app_error;

/// 日期时间能力
pub mod date_time_helper;

/// 环境变量与工作目录能力
#[cfg(feature = "runtime")]
pub mod environment_helper;

/// 统一公共错误
pub mod error_helper;

/// 网络请求扩展能力
#[cfg(feature = "runtime")]
pub mod http_client_helper;

/// 雪花标识生成能力
#[cfg(feature = "runtime")]
pub mod identifier_generator;

/// 文件系统能力
#[cfg(feature = "runtime")]
pub mod io_helper;

/// 数据转换能力
pub mod data_helper;

/// Json 处理能力
pub mod json_helper;

/// 图片处理能力
#[cfg(feature = "runtime")]
pub mod img_helper;

/// 属性处理能力
pub mod property_helper;

/// 弹性重试能力
#[cfg(feature = "runtime")]
pub mod retry_helper;

/// 字符串能力
pub mod string_helper;

/// 系统能力
#[cfg(feature = "runtime")]
pub mod system_helper;

/// 动态口令能力
#[cfg(feature = "runtime")]
pub mod totp_helper;

/// Xml 处理能力
#[cfg(feature = "runtime")]
pub mod xml_helper;

/// 调用取消令牌与可取消操作。
pub mod cancellation;

#[path = "data_helper/guid.rs"]
pub(crate) mod guid;

/// 自定义资源的异步清理登记与共享停机期限。
#[cfg(feature = "resource-cleanup")]
pub mod resource_cleanup;
/// 宿主显式资源集合。
pub mod resources;

/// 不依赖数据库驱动的连接配置。
pub mod database_settings;

/// 常驻后台服务的显式注册与宿主生命周期。
#[cfg(feature = "runtime")]
pub mod background_service;
