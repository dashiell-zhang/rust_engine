/// 授权处理器
pub mod authorize;
/// 基础工具处理器
pub mod base;
/// Demo 处理器
#[cfg(all(debug_assertions, feature = "debug-demo"))]
pub mod demo;
/// 文件处理器
pub mod file;
/// 支付处理器
pub mod pay;
/// 用户处理器
pub mod user;
