use application_interface::{
    context::{
        AppContext, AppSettingContext, AuthorizeContext, CurrentUser, FileContext, PayContext,
        RoleContext,
    },
    error::AppError,
};
use application_model::authorize::models::PermissionRouteSyncItem;
use application_service::{
    app_setting::service::AppSettingService,
    article::service::ArticleWebApiService,
    authorize::service::AuthorizeService,
    base::{BaseContext, BaseService},
    file::service::FileService,
    link::service::LinkWebApiService,
    log::service::LogManageService,
    pay::service::PayService,
    permission_route::service::PermissionRouteService,
    role::service::RoleService,
    site::service::SiteWebApiService,
    task_center::service::TaskCenterService,
    user::service::UserWebApiService,
};
use async_trait::async_trait;
use source_generator_runtime::generate_direct_service_factory;
use std::sync::Arc;
use webapi_core::host::{application::WebApiApplication, state::AppState};

/// 运行入口拥有的业务装配适配器，服务借用宿主资源的生命周期而非临时适配器。
pub struct WebApiApplicationAdapter<'a> {
    state: &'a AppState,
}
impl<'a> WebApiApplicationAdapter<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

pub trait WebApiApplicationExt {
    fn services(&self) -> WebApiApplicationAdapter<'_>;
}
impl WebApiApplicationExt for AppState {
    fn services(&self) -> WebApiApplicationAdapter<'_> {
        WebApiApplicationAdapter::new(self)
    }
}

impl<'a> WebApiApplicationAdapter<'a> {
    /// 获取主库应用层通用上下文
    pub fn primary_app_context(&self) -> AppContext<'a> {
        AppContext {
            db: self.state.primary_db(),
            id_generator: &self.state.id_generator,
            // 通用应用服务上下文默认不携带请求用户 需要时由特化上下文补充
            current_user: None,
        }
    }

    /// 获取主库 WebAPI 宿主特化上下文
    pub fn primary_webapi_context(&self) -> application_interface::context::WebApiContext<'a> {
        let app = self.primary_app_context();
        application_interface::context::WebApiContext {
            app,
            cache: Some(self.state.cache.as_ref()),
            distributed_lock: self.state.distributed_lock.as_ref(),
            file: Some(FileContext {
                app,
                file_storage: self.state.file_storage.as_deref(),
                http_client_factory: &self.state.http_client_factory,
                root_path: &self.state.settings.file.root_path,
                public_base_url: &self.state.settings.file.public_base_url,
            }),
        }
    }

    /// 显式同步当前宿主的权限路由
    pub async fn sync_permission_routes(&self) -> Result<bool, AppError> {
        let routes = self
            .state
            .permission_routes
            .iter()
            .map(|item| PermissionRouteSyncItem {
                // 权限表保存 Controller/Action 模板，HTTP 路径前导斜杠不属于权限键。
                route: item.route.trim_start_matches('/'),
                remark: item.remark,
            })
            .collect::<Vec<_>>();

        self.permission_route()
            .sync_routes(&self.state.settings.permission_module, &routes)
            .await?;

        Ok(true)
    }
}

generate_direct_service_factory! {
    impl_generics = <'a>;
    factory = WebApiApplicationAdapter<'a>;

    (self.authorize_context()) {
        authorize: AuthorizeService<'a> => dyn application_interface::services::authorize::AuthorizeService + 'a
    }

    (BaseContext { read_db: self.state.read_db(), cache: self.state.cache.as_ref() }) {
        base: BaseService<'a> => dyn application_interface::services::base::BaseService + 'a
    }

    (self.primary_app_context()) {
        task_center: TaskCenterService<'a> => dyn application_interface::services::task_center::TaskCenterService + 'a,
        permission_route: PermissionRouteService<'a> => dyn application_interface::services::permission_route::PermissionRouteService + 'a
    }

    (&self.state.log_query) {
        log_manage: LogManageService<'a> => dyn application_interface::services::log::LogManageService + 'a,
        task_logs: application_service::task_center::log_service::TaskExecutionLogService<'a> => dyn application_interface::services::task_log::TaskExecutionLogService + 'a
    }

    (self.app_setting_context()) {
        app_setting: AppSettingService<'a> => dyn application_interface::services::app_setting::AppSettingService + 'a
    }

    (self.pay_context()) {
        pay: PayService<'a> => dyn application_interface::services::pay::PayService + 'a
    }

    (self.role_context()) {
        role: RoleService<'a> => dyn application_interface::services::role::RoleService + 'a
    }

    (self.file_context()) {
        file: FileService<'a> => dyn application_interface::services::file::FileService + 'a
    }

    (self.primary_webapi_context()) {
        user_webapi: UserWebApiService<'a> => dyn application_interface::services::user::UserService + 'a,
        article_webapi: ArticleWebApiService<'a> => dyn application_interface::services::article::ArticleService + 'a,
        link_webapi: LinkWebApiService<'a> => dyn application_interface::services::link::LinkService + 'a,
        site_webapi: SiteWebApiService<'a> => dyn application_interface::services::site::SiteService + 'a
    }
}

impl<'a> WebApiApplicationAdapter<'a> {
    /// 获取应用配置模块上下文
    pub fn app_setting_context(&self) -> AppSettingContext<'a> {
        AppSettingContext {
            app: self.primary_app_context(),
            setting_lock: self.state.distributed_lock.as_ref(),
        }
    }
}

impl<'a> WebApiApplicationAdapter<'a> {
    /// 获取授权模块上下文
    pub fn authorize_context(&self) -> AuthorizeContext<'a> {
        AuthorizeContext {
            app: self.primary_app_context(),
            cache: Some(self.state.cache.as_ref()),
            authorize_lock: self.state.distributed_lock.as_ref(),
            http_client_factory: &self.state.http_client_factory,
            issuer: &self.state.settings.jwt.issuer,
            audience: &self.state.settings.jwt.audience,
            expiry_seconds: self.state.settings.jwt.expiry_seconds,
            jwt_codec: &self.state.jwt_codec,
            rsa_public_key: self
                .state
                .settings
                .rsa
                .as_ref()
                .filter(|settings| settings.public_key.is_some() || settings.private_key.is_some())
                .map(|settings| settings.public_key.as_deref()),
        }
    }
}

impl<'a> WebApiApplicationAdapter<'a> {
    /// 获取角色模块上下文
    pub fn role_context(&self) -> RoleContext<'a> {
        RoleContext {
            app: self.primary_app_context(),
            role_code_lock: self.state.distributed_lock.as_ref(),
            cache: Some(self.state.cache.as_ref()),
        }
    }
}

impl<'a> WebApiApplicationAdapter<'a> {
    /// 获取文件模块上下文
    pub fn file_context(&self) -> FileContext<'a> {
        FileContext {
            app: self.primary_app_context(),
            file_storage: self.state.file_storage.as_deref(),
            http_client_factory: &self.state.http_client_factory,
            root_path: &self.state.settings.file.root_path,
            public_base_url: &self.state.settings.file.public_base_url,
        }
    }
}

impl<'a> WebApiApplicationAdapter<'a> {
    /// 获取支付模块上下文
    pub fn pay_context(&self) -> PayContext<'a> {
        let app = self.primary_app_context();
        PayContext {
            db: app.db,
            id_generator: app.id_generator,
            current_user: app.current_user,
            cache: self.state.cache.as_ref(),
            distributed_lock: self.state.distributed_lock.as_ref(),
            http_client_factory: &self.state.http_client_factory,
        }
    }
}

/// 管理端与客户端显式选择的业务策略；公共宿主不选择具体应用服务。
pub fn application() -> Arc<dyn WebApiApplication> {
    Arc::new(BusinessWebApiApplication)
}
struct BusinessWebApiApplication;
#[async_trait]
impl WebApiApplication for BusinessWebApiApplication {
    async fn try_renew_token(
        &self,
        state: &AppState,
        user_id: i64,
        token_id: i64,
        nbf: i64,
        exp: i64,
    ) -> Result<Option<String>, AppError> {
        state
            .services()
            .authorize()
            .try_renew_token(user_id, token_id, nbf, exp)
            .await
    }
    async fn check_permission(
        &self,
        state: &AppState,
        current_user: CurrentUser,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<bool, AppError> {
        state
            .services()
            .authorize()
            .check_function_authorize(
                current_user,
                module,
                route.trim_start_matches('/'),
                allow_unbound,
            )
            .await
    }
    async fn sync_permission_routes(&self, state: &AppState) -> Result<bool, AppError> {
        state.services().sync_permission_routes().await
    }
}
