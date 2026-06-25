use application_interface::context::{
    AppContext, AppSettingContext, FileContext, MessageContext, PayContext, RoleContext,
};
use source_generator_runtime::generate_service_factory;

/// 通用应用上下文提供器
pub trait AppContextProvider {
    /// 获取应用层通用上下文
    fn app_context(&self) -> AppContext<'_>;
}

/// 角色模块上下文提供器
pub trait RoleContextProvider: AppContextProvider {
    /// 获取角色模块上下文
    fn role_context(&self) -> RoleContext<'_>;
}

/// 文件模块上下文提供器
pub trait FileContextProvider: AppContextProvider {
    /// 获取文件模块上下文
    fn file_context(&self) -> FileContext<'_>;
}

/// 消息模块上下文提供器
pub trait MessageContextProvider: AppContextProvider {
    /// 获取消息模块上下文
    fn message_context(&self) -> MessageContext<'_>;
}

/// 支付模块上下文提供器
pub trait PayContextProvider: AppContextProvider {
    /// 获取支付模块上下文
    fn pay_context(&self) -> PayContext<'_>;
}

/// 应用配置模块上下文提供器
pub trait AppSettingContextProvider: AppContextProvider {
    /// 获取应用配置模块上下文
    fn app_setting_context(&self) -> AppSettingContext<'_>;
}

/// 应用服务工厂
pub struct ApplicationServiceFactory<'a, TState> {
    /// 宿主状态引用
    state: &'a TState,
}

impl<'a, TState> ApplicationServiceFactory<'a, TState> {
    /// 创建应用服务工厂
    pub fn new(state: &'a TState) -> Self {
        Self { state }
    }
}

generate_service_factory! {
    factory = ApplicationServiceFactory<'a, TState>;
    state = TState;

    AppContextProvider => (self.state.app_context()) {
        user: crate::user::service::UserService<'a>,
        task_center: crate::task_center::service::TaskCenterService<'a>,
        log_manage: crate::log::service::LogManageService<'a>,
        article: crate::article::service::ArticleService<'a>,
        link: crate::link::service::LinkService<'a>,
        site: crate::site::service::SiteService<'a>,
        product: crate::product::service::ProductService<'a>,
        order: crate::order::service::OrderService<'a>,
        region: crate::region::service::RegionService<'a>,
        permission_route: crate::permission_route::service::PermissionRouteService<'a>
    }

    AppSettingContextProvider => (self.state.app_setting_context()) {
        app_setting: crate::app_setting::service::AppSettingService<'a>
    }

    PayContextProvider => (self.state.pay_context()) {
        pay: crate::pay::service::PayService<'a>
    }

    FileContextProvider => (self.state.file_context()) {
        file: crate::file::service::FileService<'a>
    }

    MessageContextProvider => (self.state.message_context()) {
        message: crate::message::service::MessageService<'a>
    }

    RoleContextProvider => (self.state.role_context()) {
        role: crate::role::service::RoleService<'a>
    }
}
