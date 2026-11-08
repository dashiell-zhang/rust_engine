use webapi_core::host::features::WebApiFeature;

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
