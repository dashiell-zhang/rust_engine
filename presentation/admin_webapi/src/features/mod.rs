use webapi_core::{
    auth::{HasJwtCodec, JwtDecodingState, PermissionState, TokenRenewalState},
    builder,
    host::features::WebApiFeature,
    sign_verify::sign_verify_method_router,
};

/// 基础工具特性
pub mod base;

/// 文章管理特性
pub mod article;

/// 授权特性
pub mod authorize;

/// 文件特性
pub mod file;

/// LLM 特性
pub mod llm;

/// 友情链接特性
pub mod link;

/// 日志管理特性
pub mod log;

/// 角色管理特性
pub mod role;

/// 站点管理特性
pub mod site;

/// 任务中心特性
pub mod task;

/// 用户管理特性
pub mod user;

/// UEditor 特性
pub mod ueditor;

/// 复用共享路由权限描述
pub use webapi_core::host::features::WebApiRoutePermission;

/// 为管理端单个路由同时挂载权限守卫与签名验证
pub fn protect_method_router<TState>(
    route: webapi_core::routes::MethodRouter<std::sync::Arc<TState>>,
    state: std::sync::Arc<TState>,
    module: impl Into<String>,
    route_path: impl Into<String>,
    allow_unbound: bool,
) -> webapi_core::routes::MethodRouter<std::sync::Arc<TState>>
where
    TState: HasJwtCodec
        + JwtDecodingState
        + PermissionState
        + TokenRenewalState
        + Send
        + Sync
        + 'static,
{
    // 授权过滤先执行，签名属于授权通过后的 Action 过滤阶段。
    builder::protect_method_router(
        sign_verify_method_router(route, state.clone()),
        state,
        module,
        route_path,
        allow_unbound,
    )
}

/// 获取管理端宿主启用的业务特性
pub fn features() -> Vec<Box<dyn WebApiFeature>> {
    vec![
        Box::new(authorize::AuthorizeFeature),
        Box::new(base::BaseFeature),
        Box::new(user::UserFeature),
        Box::new(role::RoleFeature),
        Box::new(task::TaskFeature),
        Box::new(log::LogFeature),
        Box::new(article::ArticleFeature),
        Box::new(file::FileFeature),
        Box::new(link::LinkFeature),
        Box::new(site::SiteFeature),
        Box::new(llm::LlmFeature),
        Box::new(ueditor::UEditorFeature),
    ]
}
