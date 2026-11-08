use sea_orm::{
    ColumnTrait, EntityTrait, PrimaryKeyTrait, QueryFilter, Select,
    sea_query::{LikeExpr, SimpleExpr},
};

/// PostgreSQL 字面子串匹配；与字符串 Contains 一致，不解释输入中的 LIKE 通配符。
pub fn contains_pattern(value: &str) -> LikeExpr {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    LikeExpr::new(format!("%{escaped}%")).escape('\\')
}

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
