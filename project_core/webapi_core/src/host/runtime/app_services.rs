use crate::auth::PermissionState;
use crate::host::state::AppState;
use application_interface::{
    context::{
        AppContext, AuthorizeContext, CurrentUser, FileContext, MessageContext, PayContext,
        RoleContext,
    },
    error::AppError,
};
use application_service::{
    app_setting::service::AppSettingService,
    article::service::{ArticleService, ArticleWebApiService},
    authorize::service::AuthorizeService,
    factory::{
        AppContextProvider, ApplicationServiceFactory, FileContextProvider, MessageContextProvider,
        PayContextProvider, RoleContextProvider,
    },
    file::service::FileService,
    link::service::{LinkService, LinkWebApiService},
    log::service::LogManageService,
    message::service::MessageService,
    order::service::OrderService,
    pay::service::PayService,
    permission_route::service::{PermissionRouteService, PermissionRouteSyncItem},
    product::service::ProductService,
    region::service::{RegionService, RegionWebApiService},
    role::service::RoleService,
    site::service::{SiteService, SiteWebApiService},
    task_center::service::TaskCenterService,
    user::service::{UserService, UserWebApiService},
};
use async_trait::async_trait;
use source_generator_runtime::generate_direct_service_factory;

const SYSTEM_QUEUE_SEND_EMAIL: &str = "System.Queue.SendEmail";

impl AppState {
    /// 获取主库应用层通用上下文
    pub fn primary_app_context(&self) -> AppContext<'_> {
        AppContext {
            db: self.primary_db(),
            id_generator: &self.id_generator,
            // 通用应用服务上下文默认不携带请求用户 需要时由特化上下文补充
            current_user: None,
        }
    }

    /// 获取主库 WebAPI 宿主特化上下文
    pub fn primary_webapi_context(&self) -> application_interface::context::WebApiContext<'_> {
        let app = self.primary_app_context();
        application_interface::context::WebApiContext {
            app,
            cache: Some(self.cache.as_ref()),
            distributed_lock: self.role_code_lock.as_ref(),
            file: Some(FileContext {
                app,
                file_storage: self.file_storage.as_deref(),
                root_path: &self.settings.file.root_path,
                public_base_url: &self.settings.file.public_base_url,
            }),
        }
    }

    /// 获取应用服务工厂
    pub fn services(&self) -> ApplicationServiceFactory<'_, Self> {
        ApplicationServiceFactory::new(self)
    }

    /// 显式同步当前宿主的权限路由
    pub async fn sync_permission_routes(&self) -> Result<bool, AppError> {
        let routes = self
            .permission_routes
            .iter()
            .map(|item| PermissionRouteSyncItem {
                route: item.route,
                remark: item.remark,
            })
            .collect::<Vec<_>>();

        self.permission_route()
            .sync_routes(&self.settings.permission_module, &routes)
            .await?;

        Ok(true)
    }
}

generate_direct_service_factory! {
    impl_generics = <>;
    factory = AppState;

    (self.authorize_context()) {
        authorize: AuthorizeService<'_>
    }

    (self.primary_app_context()) {
        user: UserService<'_>,
        task_center: TaskCenterService<'_>,
        log_manage: LogManageService<'_>,
        article: ArticleService<'_>,
        link: LinkService<'_>,
        site: SiteService<'_>,
        product: ProductService<'_>,
        order: OrderService<'_>,
        region: RegionService<'_>,
        app_setting: AppSettingService<'_>,
        permission_route: PermissionRouteService<'_>
    }

    (self.pay_context()) {
        pay: PayService<'_>
    }

    (self.role_context()) {
        role: RoleService<'_>
    }

    (self.file_context()) {
        file: FileService<'_>
    }

    (self.message_context()) {
        message: MessageService<'_>
    }

    (self.primary_webapi_context()) {
        user_webapi: UserWebApiService<'_>,
        article_webapi: ArticleWebApiService<'_>,
        link_webapi: LinkWebApiService<'_>,
        site_webapi: SiteWebApiService<'_>,
        region_webapi: RegionWebApiService<'_>
    }
}

impl AppContextProvider for AppState {
    /// 获取应用层通用上下文
    fn app_context(&self) -> AppContext<'_> {
        self.primary_app_context()
    }
}

impl AppState {
    /// 获取授权模块上下文
    fn authorize_context(&self) -> AuthorizeContext<'_> {
        AuthorizeContext {
            app: self.primary_app_context(),
            cache: Some(self.cache.as_ref()),
            authorize_lock: self.role_code_lock.as_ref(),
            http_client_factory: &self.http_client_factory,
            issuer: &self.settings.jwt.issuer,
            audience: &self.settings.jwt.audience,
            expiry_seconds: self.settings.jwt.expiry_seconds,
            jwt_codec: &self.jwt_codec,
            sms_verify_code_ttl_seconds: self.settings.sms_verify.code_ttl_seconds,
            sms_verify_resend_interval_seconds: self.settings.sms_verify.resend_interval_seconds,
            sms_queue_task_name: &self.settings.sms_verify.queue_task_name,
            sms_sign_name: &self.settings.sms_verify.sign_name,
            sms_template_code: &self.settings.sms_verify.template_code,
        }
    }
}

impl RoleContextProvider for AppState {
    /// 获取角色模块上下文
    fn role_context(&self) -> RoleContext<'_> {
        RoleContext {
            app: self.primary_app_context(),
            role_code_lock: self.role_code_lock.as_ref(),
            cache: Some(self.cache.as_ref()),
        }
    }
}

impl FileContextProvider for AppState {
    /// 获取文件模块上下文
    fn file_context(&self) -> FileContext<'_> {
        FileContext {
            app: self.primary_app_context(),
            file_storage: self.file_storage.as_deref(),
            root_path: &self.settings.file.root_path,
            public_base_url: &self.settings.file.public_base_url,
        }
    }
}

impl MessageContextProvider for AppState {
    /// 获取消息模块上下文
    fn message_context(&self) -> MessageContext<'_> {
        MessageContext {
            app: self.primary_app_context(),
            root_path: &self.settings.file.root_path,
            attachment_root_paths: None,
            email_queue_task_name: Some(SYSTEM_QUEUE_SEND_EMAIL),
            file_storage: self.file_storage.as_deref(),
            email_sender: Some(self.email_sender.as_ref()),
        }
    }
}

impl PayContextProvider for AppState {
    /// 获取支付模块上下文
    fn pay_context(&self) -> PayContext<'_> {
        let app = self.primary_app_context();
        PayContext {
            db: app.db,
            id_generator: app.id_generator,
            current_user: app.current_user,
            cache: Some(self.cache.as_ref()),
            distributed_lock: self.role_code_lock.as_ref(),
            http_client_factory: &self.http_client_factory,
        }
    }
}

#[async_trait]
impl PermissionState for AppState {
    /// 校验当前用户是否具备指定路由权限
    async fn check_permission(
        &self,
        current_user: CurrentUser,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<bool, AppError> {
        self.authorize()
            .check_function_authorize(current_user, module, route, allow_unbound)
            .await
    }
}
