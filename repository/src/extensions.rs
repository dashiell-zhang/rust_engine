pub use repository_core::extensions::{SoftDeleteEntityExt, SoftDeleteQueryExt, not_deleted_expr};

macro_rules! impl_soft_delete {
    ($module:ident) => {
        impl SoftDeleteEntityExt for crate::$module::Entity {
            fn delete_time_col() -> crate::$module::Column {
                crate::$module::Column::DeleteTime
            }
        }
    };
}

impl_soft_delete!(app_setting);
impl_soft_delete!(article);
impl_soft_delete!(category);
impl_soft_delete!(data_update_log);
impl_soft_delete!(file);
impl_soft_delete!(function);
impl_soft_delete!(function_authorize);
impl_soft_delete!(function_route);
impl_soft_delete!(link);
impl_soft_delete!(llm_app);
impl_soft_delete!(llm_model);
impl_soft_delete!(llm_conversation);
impl_soft_delete!(order);
impl_soft_delete!(order_detail);
impl_soft_delete!(product);
impl_soft_delete!(region_area);
impl_soft_delete!(region_city);
impl_soft_delete!(region_province);
impl_soft_delete!(region_town);
impl_soft_delete!(role);
impl_soft_delete!(user);
impl_soft_delete!(user_bind_external);
impl_soft_delete!(user_info);
impl_soft_delete!(user_role);
impl_soft_delete!(user_token);
