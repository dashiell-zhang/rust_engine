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

/// 后台入口选择的消息服务；每次执行取得新的应用服务实例。
pub trait TaskServiceFactory: Send + Sync {
    fn message(&self) -> Box<dyn message::MessageService + '_>;
    fn sms(&self) -> Box<dyn sms::SmsService + '_>;
}
