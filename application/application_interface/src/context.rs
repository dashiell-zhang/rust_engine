use distributed_cache::DistributedCache;
use distributed_lock::DistributedLock;
use identifier_generator::IdentifierGenerator;
use sea_orm::DatabaseConnection;

use crate::auth::EcJwtCodec;
use crate::email::AppEmailSender;
use crate::http_client::HttpClientFactory;
use crate::llm::AppLlmClientFactory;
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
    pub cache: Option<&'a dyn DistributedCache>,

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

/// 应用层当前用户上下文
#[derive(Clone, Copy)]
pub struct CurrentUser {
    /// 当前用户 ID
    pub user_id: i64,
    /// 当前 Token ID
    pub token_id: Option<i64>,
}

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
    /// 验证码缓存秒数
    pub sms_verify_code_ttl_seconds: u64,
    /// 验证码重发限制秒数
    pub sms_verify_resend_interval_seconds: u64,
    /// 短信队列任务名称
    pub sms_queue_task_name: &'a str,
    /// 短信签名
    pub sms_sign_name: &'a str,
    /// 短信模板编号
    pub sms_template_code: &'a str,
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

    /// 文件根目录
    pub root_path: &'a str,

    /// 附件附加根目录列表
    /// TaskService 可配置多个宿主的本地存储根目录用于回溯附件
    pub attachment_root_paths: Option<&'a Vec<String>>,

    /// 邮件队列任务名称
    pub email_queue_task_name: Option<&'a str>,

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

    /// LLM 客户端工厂
    pub llm_client_factory: &'a dyn AppLlmClientFactory,
}
