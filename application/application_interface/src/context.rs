use distributed_cache::DistributedCache;
use distributed_lock::DistributedLock;
use identifier_generator::IdentifierGenerator;
use sea_orm::DatabaseConnection;

use crate::auth::EcJwtCodec;
use crate::email::AppEmailSender;
use crate::http_client::HttpClientFactory;
use crate::sms::AppSmsSender;
use crate::storage::AppFileStorage;

/// 应用层通用上下文
#[derive(Clone, Copy)]
pub struct AppContext<'a> {
    /// 数据库连接
    pub db: &'a DatabaseConnection,

    /// ID 生成器
    pub id_generator: &'a IdentifierGenerator,

    /// 当前用户
    /// 宿主未处于登录态或在后台系统流程中调用时为 None
    pub current_user: Option<CurrentUser>,
}

/// 应用配置模块上下文
#[derive(Clone, Copy)]
pub struct AppSettingContext<'a> {
    /// 通用上下文
    pub app: AppContext<'a>,

    /// 配置写入锁
    pub setting_lock: &'a dyn DistributedLock,
}

/// WebAPI 宿主特化上下文
#[derive(Clone, Copy)]
pub struct WebApiContext<'a> {
    /// 通用上下文
    pub app: AppContext<'a>,
    /// 缓存
    pub cache: Option<&'a dyn DistributedCache>,
    /// 宿主级分布式锁
    pub distributed_lock: &'a dyn DistributedLock,
    /// 文件模块上下文
    pub file: Option<FileContext<'a>>,
}

/// 支付模块上下文
#[derive(Clone, Copy)]
pub struct PayContext<'a> {
    /// 数据库连接
    pub db: &'a DatabaseConnection,

    /// ID 生成器
    pub id_generator: &'a IdentifierGenerator,

    /// 当前用户
    pub current_user: Option<CurrentUser>,

    /// 缓存
    pub cache: &'a dyn DistributedCache,

    /// 分布式锁
    pub distributed_lock: &'a dyn DistributedLock,

    /// 共享 HTTP 客户端工厂
    pub http_client_factory: &'a HttpClientFactory,
}

impl<'a> PayContext<'a> {
    /// 转换为通用应用上下文
    pub fn app_context(&self) -> AppContext<'a> {
        AppContext {
            db: self.db,
            id_generator: self.id_generator,
            current_user: self.current_user,
        }
    }
}

pub use application_model::shared::current_user::CurrentUser;

/// 应用层授权上下文
#[derive(Clone, Copy)]
pub struct AuthorizeContext<'a> {
    /// 通用上下文
    pub app: AppContext<'a>,
    /// 缓存
    pub cache: Option<&'a dyn DistributedCache>,
    /// 授权相关分布式锁
    pub authorize_lock: &'a dyn DistributedLock,
    /// 共享 HTTP 客户端工厂
    pub http_client_factory: &'a HttpClientFactory,
    /// Token 颁发者
    pub issuer: &'a str,
    /// Token 受众
    pub audience: &'a str,
    /// Token 有效期
    pub expiry_seconds: i64,
    /// JWT 编解码器
    pub jwt_codec: &'a EcJwtCodec,
    /// RSA 配置中的公钥；外层 None 表示配置段不存在，内层 None 表示未配置公钥。
    pub rsa_public_key: Option<Option<&'a str>>,
}

/// 角色模块上下文
#[derive(Clone, Copy)]
pub struct RoleContext<'a> {
    /// 通用上下文
    pub app: AppContext<'a>,
    /// 角色编码锁
    pub role_code_lock: &'a dyn DistributedLock,
    /// 缓存
    pub cache: Option<&'a dyn DistributedCache>,
}

/// 文件模块上下文
#[derive(Clone, Copy)]
pub struct FileContext<'a> {
    /// 通用上下文
    pub app: AppContext<'a>,
    /// 文件存储
    pub file_storage: Option<&'a dyn AppFileStorage>,
    /// 共享 HTTP 客户端工厂
    pub http_client_factory: &'a HttpClientFactory,
    /// 文件根目录
    pub root_path: &'a str,
    /// 公共访问基地址
    pub public_base_url: &'a str,
}

/// 短信模块上下文
#[derive(Clone, Copy)]
pub struct SmsContext<'a> {
    /// 短信发送器
    pub sms_sender: &'a dyn AppSmsSender,
}

/// 邮件模块上下文
#[derive(Clone, Copy)]
pub struct MessageContext<'a> {
    /// 通用上下文
    pub app: AppContext<'a>,

    /// 宿主启动时的内容目录，不使用文件提供商的存储根目录
    pub root_path: &'a std::path::Path,

    /// 文件存储（可选，仅在需要发送附件时传入）
    pub file_storage: Option<&'a dyn AppFileStorage>,

    /// 邮件发送器（可选，未配置时发送邮件将返回错误）
    pub email_sender: Option<&'a dyn AppEmailSender>,
}

/// LLM 模块上下文
#[derive(Clone, Copy)]
pub struct LlmContext<'a> {
    /// 通用上下文
    pub app: AppContext<'a>,

    /// LLM 配置并发控制锁
    pub distributed_lock: &'a dyn DistributedLock,

    /// 共享 HTTP 客户端工厂
    pub http_client_factory: &'a HttpClientFactory,
}
