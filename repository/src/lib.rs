//! 仓储层实体与 schema 定义

pub use extensions::SoftDeleteEntityExt;

/// 业务写模型在 public 中的独立迁移历史表。
pub const MIGRATION_HISTORY_TABLE: &str = "__EFMigrationsHistory_Business";

/// 仓储查询扩展
pub mod extensions;

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

/// 订单实体
pub mod order;

/// 订单明细实体
pub mod order_detail;

/// 商品实体
pub mod product;

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

// 当前模型没有 JSON 索引声明，但仍参与工具拥有的旧索引差异检查。
repository_core::impl_json_index_model!(
    app_setting::Entity,
    article::Entity,
    category::Entity,
    data_update_log::Entity,
    file::Entity,
    function::Entity,
    function_authorize::Entity,
    function_route::Entity,
    link::Entity,
    llm_app::Entity,
    llm_model::Entity,
    llm_conversation::Entity,
    order::Entity,
    order_detail::Entity,
    product::Entity,
    region_area::Entity,
    region_city::Entity,
    region_province::Entity,
    region_town::Entity,
    role::Entity,
    user::Entity,
    user_bind_external::Entity,
    user_info::Entity,
    user_role::Entity,
    user_token::Entity,
);

/// 当前模型迁移元数据。
pub mod schema;

/// 模板业务模型的固定身份，由宿主为该类型选择一个数据库目标。
pub struct BusinessModule;
impl repository_core::module::DatabaseModule for BusinessModule {
    const NAME: &'static str = "business";
    const MIGRATION_HISTORY_TABLE: &'static str = MIGRATION_HISTORY_TABLE;
}
