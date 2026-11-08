//! 请求处理器和权限适配器依赖的业务用例契约。
//! 实现由运行入口选择；参数与结果只使用模型或通用类型。

pub mod app_setting;
pub mod article;
pub mod authorize;
pub mod base;
pub mod file;
pub mod link;
pub mod llm_app;
pub mod llm_conversation;
pub mod llm_invoke;
pub mod llm_model;
pub mod log;
pub mod message;
pub mod pay;
pub mod permission_route;
pub mod region;
pub mod role;
pub mod site;
pub mod sms;
pub mod task_center;
pub mod task_log;
pub mod user;

/// 入口选择的业务服务工厂；只返回业务契约，不暴露宿主状态或数据库。
pub trait ServiceFactory<'a>: Send + Sync {
    fn authorize(&self) -> Box<dyn authorize::AuthorizeService + 'a>;
    fn base(&self) -> Box<dyn base::BaseService + 'a>;
    fn task_center(&self) -> Box<dyn task_center::TaskCenterService + 'a>;
    fn permission_route(&self) -> Box<dyn permission_route::PermissionRouteService + 'a>;
    fn log_manage(&self) -> Box<dyn log::LogManageService + 'a>;
    fn task_logs(&self) -> Box<dyn task_log::TaskExecutionLogService + 'a>;
    fn app_setting(&self) -> Box<dyn app_setting::AppSettingService + 'a>;
    fn pay(&self) -> Box<dyn pay::PayService + 'a>;
    fn role(&self) -> Box<dyn role::RoleService + 'a>;
    fn file(&self) -> Box<dyn file::FileService + 'a>;
    fn user_webapi(&self) -> Box<dyn user::UserService + 'a>;
    fn article_webapi(&self) -> Box<dyn article::ArticleService + 'a>;
    fn link_webapi(&self) -> Box<dyn link::LinkService + 'a>;
    fn site_webapi(&self) -> Box<dyn site::SiteService + 'a>;
    fn llm_app(&self) -> Box<dyn llm_app::LlmAppService + 'a>;
    fn llm_model(&self) -> Box<dyn llm_model::LlmModelService + 'a>;
    fn llm_invoke(&self) -> Box<dyn llm_invoke::LlmInvokeService + 'a>;
    fn llm_conversation_manage(
        &self,
    ) -> Box<dyn llm_conversation::LlmConversationManageService + 'a>;
}

/// 后台入口选择的消息服务；每次执行取得新的应用服务实例。
pub trait TaskServiceFactory: Send + Sync {
    fn message(&self) -> Box<dyn message::MessageService + '_>;
    fn sms(&self) -> Box<dyn sms::SmsService + '_>;
}
