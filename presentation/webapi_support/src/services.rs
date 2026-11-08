//! 按业务能力独立注册的工厂；构造只在 presentation 选择。
use super::{
    WebApiApplicationAdapter,
    resources::{BusinessResources, LogQueryResource},
};
use application_interface::error::AppError;
use webapi_core::host::{resources::WebApiResources, state::AppState};

pub trait ServiceKey: Send + Sync + 'static {
    type Service<'a>: ?Sized + 'a;
}
pub type ServiceConstructor<K> =
    for<'a> fn(&'a AppState) -> Result<Box<<K as ServiceKey>::Service<'a>>, AppError>;
pub struct Factory<K: ServiceKey> {
    create: ServiceConstructor<K>,
    validate: fn(&WebApiResources) -> Result<(), String>,
}
/// 注册单项能力，拒绝覆盖；替换实现时直接选择自定义构造函数。
pub fn register<K: ServiceKey>(
    resources: &mut WebApiResources,
    create: ServiceConstructor<K>,
    validate: fn(&WebApiResources) -> Result<(), String>,
) -> Result<(), String> {
    validate(resources)?;
    resources.insert(Factory::<K> { create, validate })
}
pub fn require<K: ServiceKey>(resources: &WebApiResources) -> Result<(), String> {
    (resources.get::<Factory<K>>()?.validate)(resources)
}
pub struct Services<'a> {
    pub(super) state: &'a AppState,
}
impl<'a> Services<'a> {
    pub fn get<K: ServiceKey>(&self) -> Result<Box<K::Service<'a>>, AppError> {
        let factory = self
            .state
            .resources
            .get::<Factory<K>>()
            .map_err(AppError::internal)?;
        (factory.create)(self.state)
    }
}
fn validate_business(resources: &WebApiResources) -> Result<(), String> {
    resources
        .get::<BusinessResources>()?
        .modules
        .get::<repository::BusinessModule>()?;
    Ok(())
}
fn validate_file_business(resources: &WebApiResources) -> Result<(), String> {
    validate_business(resources)?;
    resources.get::<super::resources::FileStorageResource>()?;
    Ok(())
}
fn validate_logs(resources: &WebApiResources) -> Result<(), String> {
    resources.get::<LogQueryResource>()?;
    Ok(())
}
#[cfg(feature = "llm")]
fn validate_llm(resources: &WebApiResources) -> Result<(), String> {
    validate_business(resources)?;
    resources.get::<super::llm_resources::LlmClientFactoryResource>()?;
    resources.get::<super::llm_resources::LlmModelConfigResource>()?;
    Ok(())
}
// 声明键和快捷访问方法，不生成默认实现或全量注册。
macro_rules! service_keys {
    ($( $key:ident, $method:ident => $contract:path; )*) => { $(
        pub struct $key;
        impl ServiceKey for $key { type Service<'a> = dyn $contract + 'a; }
        impl<'a> Services<'a> { pub fn $method(&self) -> Result<Box<dyn $contract + 'a>, AppError> { self.get::<$key>() } }
    )* };
}
service_keys! {
    Authorize, authorize => application_interface::services::authorize::AuthorizeService;
    Base, base => application_interface::services::base::BaseService;
    TaskCenter, task_center => application_interface::services::task_center::TaskCenterService;
    PermissionRoute, permission_route => application_interface::services::permission_route::PermissionRouteService;
    LogManage, log_manage => application_interface::services::log::LogManageService;
    TaskLogs, task_logs => application_interface::services::task_log::TaskExecutionLogService;
    AppSetting, app_setting => application_interface::services::app_setting::AppSettingService;
    Pay, pay => application_interface::services::pay::PayService;
    Role, role => application_interface::services::role::RoleService;
    File, file => application_interface::services::file::FileService;
    User, user_webapi => application_interface::services::user::UserService;
    Article, article_webapi => application_interface::services::article::ArticleService;
    Link, link_webapi => application_interface::services::link::LinkService;
    Site, site_webapi => application_interface::services::site::SiteService;
    LlmApp, llm_app => application_interface::services::llm_app::LlmAppService;
    LlmModel, llm_model => application_interface::services::llm_model::LlmModelService;
    LlmInvoke, llm_invoke => application_interface::services::llm_invoke::LlmInvokeService;
    LlmConversation, llm_conversation_manage => application_interface::services::llm_conversation::LlmConversationManageService;
}
// 默认服务构造继续复用已有 generate_direct_service_factory；这里只声明各能力的注册入口。
macro_rules! default_factories {
    ($( $key:ident, $method:ident, $validate:ident; )*) => { $(
        impl $key {
            pub fn register_default(resources: &mut WebApiResources) -> Result<(), String> {
                register::<Self>(resources, |state| Ok(WebApiApplicationAdapter::new(state).$method()), $validate)
            }
        }
    )* };
}
default_factories! {
    Authorize, authorize, validate_business;
    Base, base, validate_business;
    TaskCenter, task_center, validate_business;
    PermissionRoute, permission_route, validate_business;
    LogManage, log_manage, validate_logs;
    TaskLogs, task_logs, validate_logs;
    AppSetting, app_setting, validate_business;
    Pay, pay, validate_business;
    Role, role, validate_business;
    File, file, validate_file_business;
    User, user_webapi, validate_file_business;
    Article, article_webapi, validate_file_business;
    Link, link_webapi, validate_file_business;
    Site, site_webapi, validate_file_business;
}
#[cfg(feature = "llm")]
default_factories! {
    LlmApp, llm_app, validate_llm;
    LlmModel, llm_model, validate_business;
    LlmInvoke, llm_invoke, validate_llm;
    LlmConversation, llm_conversation_manage, validate_business;
}
