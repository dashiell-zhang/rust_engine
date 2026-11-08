use queue_task_database::queue_task;
use queue_task_database::task_setting;
use repository::{
    app_setting, article, category, data_update_log, file, function, function_authorize,
    function_route, link, llm_app, llm_conversation, llm_model, order, order_detail, product,
    region_area, region_city, region_province, region_town, role, user, user_bind_external,
    user_info, user_role, user_token,
};

// 当前业务模型的索引比较清单；历史迁移独立保存，不随这里的实体声明变化。
macro_rules! business_entities {
    ($callback:ident $(, $argument:expr)*) => {
        $callback($($argument,)* user::Entity);
        $callback($($argument,)* role::Entity);
        $callback($($argument,)* function::Entity);
        $callback($($argument,)* region_province::Entity);
        $callback($($argument,)* product::Entity);
        $callback($($argument,)* region_city::Entity);
        $callback($($argument,)* region_area::Entity);
        $callback($($argument,)* region_town::Entity);
        $callback($($argument,)* category::Entity);
        $callback($($argument,)* function_authorize::Entity);
        $callback($($argument,)* function_route::Entity);
        $callback($($argument,)* user_role::Entity);
        $callback($($argument,)* user_token::Entity);
        $callback($($argument,)* task_setting::Entity);
        $callback($($argument,)* queue_task::Entity);
        $callback($($argument,)* file::Entity);
        $callback($($argument,)* user_info::Entity);
        $callback($($argument,)* article::Entity);
        $callback($($argument,)* link::Entity);
        $callback($($argument,)* data_update_log::Entity);
        $callback($($argument,)* app_setting::Entity);
        $callback($($argument,)* llm_model::Entity);
        $callback($($argument,)* llm_app::Entity);
        $callback($($argument,)* llm_conversation::Entity);
        $callback($($argument,)* order::Entity);
        $callback($($argument,)* order_detail::Entity);
        $callback($($argument,)* user_bind_external::Entity);
    };
}

/// 当前写模型拥有的所有表；索引声明及实际映射由实体所属 crate 提供。
pub fn json_index_tables() -> Vec<repository_core::json_index::JsonIndexTable> {
    let mut tables = Vec::new();
    business_entities!(append_json_index_model, &mut tables);
    tables
}
fn append_json_index_model<E: repository_core::json_index::JsonIndexedEntity>(
    tables: &mut Vec<repository_core::json_index::JsonIndexTable>,
    _: E,
) {
    tables.push(E::json_index_model());
}

/// 当前业务关系模型，只供离线生成新迁移；已登记迁移不调用本函数。
pub fn model_snapshot() -> Result<repository_core::migration::model::ModelSnapshot, String> {
    use repository_core::migration::model::{ModelSnapshot, TableModel};
    let mut model = ModelSnapshot::default();
    fn append_table<E: sea_orm::EntityTrait>(
        tables: &mut Vec<Result<TableModel, String>>,
        entity: E,
    ) {
        tables.push(TableModel::from_entity(entity));
    }
    let mut tables = Vec::new();
    business_entities!(append_table, &mut tables);
    for table in tables {
        let mut table = table?;
        table.add_audit_indexes()?;
        model.add_table(table)?;
    }
    model.extensions.insert("citext".into());
    // 当前 NetEngine 实体上的 Index 声明；常规关系和审计索引由公共模型补齐。
    for (table, column) in repository::schema::declared_indexes() {
        model
            .tables
            .get_mut(&format!("\"{table}\""))
            .ok_or("索引所属表未登记")?
            .add_index(&[*column], false)?;
    }
    for (table, name, sql) in repository::schema::indexes()
        .iter()
        .chain(queue_task_database::schema::indexes())
    {
        let key = format!("\"{table}\"");
        model
            .tables
            .get_mut(&key)
            .ok_or_else(|| format!("索引所属表未登记：{table}"))?
            .indexes
            .insert((*name).into(), (*sql).into());
    }
    Ok(model)
}
