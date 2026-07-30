//! 仓储层实体与 schema 定义

pub use extensions::SoftDeleteEntityExt;

/// 数据库健康检查工具
pub mod database_health;

/// 仓储查询扩展
pub mod extensions;

/// SQL 命令策略工具
pub mod sql_command;

/// 数据库只读连接
pub mod read_database;

/// 应用配置实体
pub mod app_setting;

/// 文章实体
pub mod article;

/// 文章分类实体
pub mod category;

/// 数据变更日志实体
pub mod data_update_log;

/// 文件实体
pub mod file;

/// 功能节点实体
pub mod function;

/// 功能授权实体
pub mod function_authorize;

/// 功能路由绑定实体
pub mod function_route;

/// 友情链接实体
pub mod link;

/// LLM 应用配置实体
pub mod llm_app;

/// LLM 模型配置实体
pub mod llm_model;

/// LLM 对话记录实体
pub mod llm_conversation;

/// 日志实体
pub mod log;

/// 订单实体
pub mod order;

/// 订单明细实体
pub mod order_detail;

/// 商品实体
pub mod product;

/// 队列任务实体
pub mod queue_task;

/// 区县字典实体
pub mod region_area;

/// 城市字典实体
pub mod region_city;

/// 省份字典实体
pub mod region_province;

/// 乡镇字典实体
pub mod region_town;

/// 角色实体
pub mod role;

/// 任务配置实体
pub mod task_setting;

/// 用户实体
pub mod user;

/// 用户外部账号绑定实体
pub mod user_bind_external;

/// 用户资料实体
pub mod user_info;

/// 用户角色关系实体
pub mod user_role;

/// 用户 Token 实体
pub mod user_token;
