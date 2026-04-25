use webapi_core::host::features::WebApiFeature;

/// 授权特性
pub mod authorize;
/// 基础工具特性
pub mod base;

/// Demo 特性
#[cfg(all(debug_assertions, feature = "debug-demo"))]
pub mod demo;

/// 文件特性
pub mod file;

/// 支付特性
pub mod pay;

/// 用户管理特性
pub mod user;

/// 复用共享路由权限描述
pub use webapi_core::host::features::WebApiRoutePermission;

/// 获取客户端宿主启用的业务特性
pub fn features() -> Vec<Box<dyn WebApiFeature>> {
    #[allow(unused_mut)]
    let mut features: Vec<Box<dyn WebApiFeature>> = vec![
        Box::new(authorize::AuthorizeFeature),
        Box::new(base::BaseFeature),
        Box::new(user::UserFeature),
        Box::new(file::FileFeature),
        Box::new(pay::PayFeature),
    ];

    #[cfg(all(debug_assertions, feature = "debug-demo"))]
    {
        features.push(Box::new(demo::DemoFeature));
    }

    features
}
