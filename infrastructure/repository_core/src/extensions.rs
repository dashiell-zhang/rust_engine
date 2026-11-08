use sea_orm::{
    ColumnTrait, EntityTrait, JoinType, PrimaryKeyTrait, QueryFilter, QuerySelect, Related, Select,
    SelectTwo,
    sea_query::{Condition, Expr, ExprTrait, LikeExpr, SimpleExpr},
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

/// 对关联目标追加活跃过滤，放入 ON 保留左侧记录，不能用 WHERE 代替。
/// 多对多中间实体的可见性必须由调用者另行声明，不猜测中间表的删除列。
pub trait SoftDeleteSelectExt<E: EntityTrait>: Sized {
    fn find_also_related_active<R>(self, related: R) -> SelectTwo<E, R>
    where
        R: SoftDeleteEntityExt,
        E: Related<R>;
}

impl<E: EntityTrait> SoftDeleteSelectExt<E> for Select<E> {
    fn find_also_related_active<R>(mut self, related: R) -> SelectTwo<E, R>
    where
        R: SoftDeleteEntityExt,
        E: Related<R>,
    {
        if let Some(via) = E::via() {
            self = self.join(JoinType::LeftJoin, via);
        }
        let relation = E::to();
        let table = relation.to_tbl.clone();
        let target = table
            .sea_orm_table_alias()
            .unwrap_or_else(|| table.sea_orm_table());
        let condition = Condition::all()
            .add(Condition::from(relation))
            .add(Expr::col((target.clone(), R::delete_time_col())).is_null());
        QuerySelect::query(&mut self).join(JoinType::LeftJoin, table, condition);
        self.select_also(related)
    }
}

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
/// SoftDelete 派生的固有查询入口复用此过滤；SeaORM 泛型/原始 SQL 仍须显式选择。
pub trait SoftDeleteEntityExt: EntityTrait {
    /// 返回当前实体的软删除字段列
    fn delete_time_col() -> Self::Column;

    /// 查询未软删除的记录
    fn find_active() -> Select<Self> {
        <Self as EntityTrait>::find().filter(Self::delete_time_col().is_null())
    }

    /// 按主键查询未软删除的记录
    fn find_by_id_active<V>(id: V) -> Select<Self>
    where
        V: Into<<Self::PrimaryKey as PrimaryKeyTrait>::ValueType>,
    {
        <Self as EntityTrait>::find_by_id(id).filter(Self::delete_time_col().is_null())
    }
}
