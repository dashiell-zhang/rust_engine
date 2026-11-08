//! PostgreSQL 有符号分页：保留调用方的偏移，不把负数转换为第一页。
use sea_orm::{
    DbBackend, DbErr, EntityTrait, QuerySelect, QueryTrait, Select, SelectModel, SelectorRaw,
};

pub trait SignedPage<E: EntityTrait> {
    fn signed_page(self, skip: i32, take: i32) -> SelectorRaw<SelectModel<E::Model>>;
}

impl<E: EntityTrait> SignedPage<E> for Select<E> {
    fn signed_page(mut self, skip: i32, take: i32) -> SelectorRaw<SelectModel<E::Model>> {
        // SeaQuery 的分页接口只接受 u64。保留查询生成的 SQL 和绑定参数，
        // 仅以已解析的整数追加分页，负偏移由 PostgreSQL 在执行列表查询时拒绝。
        QuerySelect::query(&mut self).reset_limit().reset_offset();
        let mut statement = self.build(DbBackend::Postgres);
        statement
            .sql
            .push_str(&format!(" LIMIT {take} OFFSET {skip}"));
        self.from_raw_sql(statement)
    }
}

/// 与 CountAsync 的 Int32 返回范围一致，避免静默截断总数。
pub fn count_i32(count: u64) -> Result<i32, DbErr> {
    i32::try_from(count).map_err(|_| DbErr::Custom("记录总数超出 Int32 范围".into()))
}
