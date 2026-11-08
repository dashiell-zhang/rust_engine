/// 应用层错误定义
pub use common::app_error as error;

/// 应用层上下文定义
pub use application_runtime::context;

/// 应用配置管理模块应用服务
pub mod app_setting;

/// 授权模块应用服务
pub mod authorize;

/// 应用服务工厂
pub mod factory;

/// 应用层写操作辅助
pub mod write_support;

/// 角色模块应用服务
pub mod role;

/// 用户模块应用服务
pub mod user;

/// 任务中心模块应用服务
pub mod task_center;

/// 日志管理模块应用服务
pub mod log;

/// 文章与栏目模块应用服务
pub mod article;

/// 友情链接模块应用服务
pub mod link;

/// 文件模块应用服务
pub mod file;

/// 支付模块应用服务
pub mod pay;

/// 权限路由应用服务
pub mod permission_route;

/// 站点模块应用服务
pub mod site;

/// 商品模块应用服务
pub mod product;

/// 订单管理模块应用服务
pub mod order;

/// 地区字典模块应用服务
pub mod region;

/// 只读基础数据与图形验证码应用服务
pub mod base;

/// 消息模块应用服务
pub mod message;

mod contracts;
