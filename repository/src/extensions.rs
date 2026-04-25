use sea_orm::{
    ColumnTrait, EntityTrait, PrimaryKeyTrait, QueryFilter, Select, sea_query::SimpleExpr,
};

/// SeaORM 软删除查询扩展
/// 为支持查询过滤的类型提供 not_deleted 链式方法
pub trait SoftDeleteQueryExt: QueryFilter + Sized {
    /// 追加未删除数据过滤条件
    fn not_deleted<C>(self, delete_time_column: C) -> Self
    where
        C: ColumnTrait,
    {
        self.filter(delete_time_column.is_null())
    }
}

/// 为支持查询过滤的类型提供软删除扩展
impl<T> SoftDeleteQueryExt for T where T: QueryFilter + Sized {}

/// 返回软删除过滤表达式
/// 适用于 Condition 组合场景
pub fn not_deleted_expr<C>(column: C) -> SimpleExpr
where
    C: ColumnTrait,
{
    column.is_null()
}

/// 软删除实体扩展
/// 为具有软删除字段的实体提供统一的活跃记录查询方法
/// 对应 NetEngine 中 OnModelCreating 的全局 ApplySoftDeleteFilters 效果
pub trait SoftDeleteEntityExt: EntityTrait {
    /// 返回当前实体的软删除字段列
    fn delete_time_col() -> Self::Column;

    /// 查询未软删除的记录
    fn find_active() -> Select<Self> {
        Self::find().filter(Self::delete_time_col().is_null())
    }

    /// 按主键查询未软删除的记录
    fn find_by_id_active<V>(id: V) -> Select<Self>
    where
        V: Into<<Self::PrimaryKey as PrimaryKeyTrait>::ValueType>,
    {
        Self::find_by_id(id).filter(Self::delete_time_col().is_null())
    }
}

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
impl_soft_delete!(llm_conversation);
impl_soft_delete!(log);
impl_soft_delete!(order);
impl_soft_delete!(order_detail);
impl_soft_delete!(product);
impl_soft_delete!(queue_task);
impl_soft_delete!(region_area);
impl_soft_delete!(region_city);
impl_soft_delete!(region_province);
impl_soft_delete!(region_town);
impl_soft_delete!(role);
impl_soft_delete!(task_setting);
impl_soft_delete!(user);
impl_soft_delete!(user_bind_external);
impl_soft_delete!(user_info);
impl_soft_delete!(user_role);
impl_soft_delete!(user_token);
