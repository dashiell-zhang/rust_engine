use sea_orm::{
    ConditionalStatement, DbBackend, EntityTrait, ExprTrait, Schema,
    sea_query::{ColumnDef, Expr, Index, Table, TableAlterStatement},
};

use repository::{
    app_setting, article, category, data_update_log, file, function, function_authorize,
    function_route, link, llm_app, llm_conversation, llm_model, log, order, order_detail, product,
    queue_task, region_area, region_city, region_province, region_town, role, task_setting, user,
    user_bind_external, user_info, user_role, user_token,
};

/// 获取完整 schema 初始化语句
pub fn initial_schema_statements(backend: DbBackend) -> Vec<String> {
    let schema = Schema::new(backend);
    let mut statements = vec![];

    append_entity_table(&mut statements, backend, &schema, user::Entity);
    append_entity_table(&mut statements, backend, &schema, role::Entity);
    append_entity_table(&mut statements, backend, &schema, function::Entity);
    append_entity_table(&mut statements, backend, &schema, region_province::Entity);
    append_entity_table(&mut statements, backend, &schema, product::Entity);
    append_entity_table(&mut statements, backend, &schema, region_city::Entity);
    append_entity_table(&mut statements, backend, &schema, region_area::Entity);
    append_entity_table(&mut statements, backend, &schema, region_town::Entity);
    append_entity_table(&mut statements, backend, &schema, category::Entity);
    append_entity_table(
        &mut statements,
        backend,
        &schema,
        function_authorize::Entity,
    );
    append_entity_table(&mut statements, backend, &schema, function_route::Entity);
    append_entity_table(&mut statements, backend, &schema, user_role::Entity);
    append_entity_table(&mut statements, backend, &schema, user_token::Entity);
    append_entity_table(&mut statements, backend, &schema, task_setting::Entity);
    append_entity_table(&mut statements, backend, &schema, queue_task::Entity);
    append_entity_table(&mut statements, backend, &schema, file::Entity);
    append_entity_table(&mut statements, backend, &schema, log::Entity);
    append_entity_table(&mut statements, backend, &schema, user_info::Entity);
    append_entity_table(&mut statements, backend, &schema, article::Entity);
    append_entity_table(&mut statements, backend, &schema, link::Entity);
    append_entity_table(&mut statements, backend, &schema, data_update_log::Entity);
    append_entity_table(&mut statements, backend, &schema, app_setting::Entity);
    append_entity_table(&mut statements, backend, &schema, llm_model::Entity);
    append_entity_table(&mut statements, backend, &schema, llm_app::Entity);
    append_entity_table(&mut statements, backend, &schema, llm_conversation::Entity);
    append_entity_table(&mut statements, backend, &schema, order::Entity);
    append_entity_table(&mut statements, backend, &schema, order_detail::Entity);
    append_entity_table(
        &mut statements,
        backend,
        &schema,
        user_bind_external::Entity,
    );

    statements.extend(schema_index_statements(backend));
    statements.extend(schema_patch_statements(backend));

    statements
}

/// 获取旧 schema 对齐语句
pub fn legacy_alignment_statements(backend: DbBackend) -> Vec<String> {
    let mut statements = vec![];

    append_missing_entity_table(&mut statements, backend, product::Entity);
    append_missing_entity_table(&mut statements, backend, order::Entity);
    append_missing_entity_table(&mut statements, backend, order_detail::Entity);
    append_missing_entity_table(&mut statements, backend, llm_model::Entity);
    append_missing_entity_table(&mut statements, backend, llm_conversation::Entity);
    append_missing_entity_table(&mut statements, backend, user_bind_external::Entity);

    statements.extend(schema_index_statements(backend));
    statements.extend(schema_patch_statements(backend));

    statements
}

/// 获取 LLM 模型配置结构迁移语句
pub fn llm_model_alignment_statements(backend: DbBackend) -> Vec<String> {
    let mut statements = vec![];

    append_missing_entity_table(&mut statements, backend, llm_model::Entity);
    statements.extend(schema_index_statements(backend));
    statements.extend(schema_patch_statements(backend));

    statements
}

/// 获取 schema 索引语句
fn schema_index_statements(backend: DbBackend) -> Vec<String> {
    vec![
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_User_UserName_NotDeleted")
                .table(user::Entity)
                .col(user::Column::UserName)
                .unique()
                .and_where(Expr::col((user::Entity, user::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Role_Code_NotDeleted")
                .table(role::Entity)
                .col(role::Column::Code)
                .unique()
                .and_where(
                    Expr::col((role::Entity, role::Column::Code))
                        .is_not_null()
                        .and(Expr::col((role::Entity, role::Column::DeleteTime)).is_null()),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_UserRole_UserId_RoleId_NotDeleted")
                .table(user_role::Entity)
                .col(user_role::Column::UserId)
                .col(user_role::Column::RoleId)
                .unique()
                .and_where(Expr::col((user_role::Entity, user_role::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Function_Sign_NotDeleted")
                .table(function::Entity)
                .col(function::Column::Sign)
                .unique()
                .and_where(Expr::col((function::Entity, function::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Function_ParentId_NotDeleted")
                .table(function::Entity)
                .col(function::Column::ParentId)
                .and_where(Expr::col((function::Entity, function::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_FunctionAuthorize_Role_Function_NotDeleted")
                .table(function_authorize::Entity)
                .col(function_authorize::Column::FunctionId)
                .col(function_authorize::Column::RoleId)
                .unique()
                .and_where(
                    Expr::col((
                        function_authorize::Entity,
                        function_authorize::Column::RoleId,
                    ))
                    .is_not_null()
                    .and(
                        Expr::col((
                            function_authorize::Entity,
                            function_authorize::Column::DeleteTime,
                        ))
                        .is_null(),
                    ),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_FunctionAuthorize_User_Function_NotDeleted")
                .table(function_authorize::Entity)
                .col(function_authorize::Column::FunctionId)
                .col(function_authorize::Column::UserId)
                .unique()
                .and_where(
                    Expr::col((
                        function_authorize::Entity,
                        function_authorize::Column::UserId,
                    ))
                    .is_not_null()
                    .and(
                        Expr::col((
                            function_authorize::Entity,
                            function_authorize::Column::DeleteTime,
                        ))
                        .is_null(),
                    ),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_FunctionRoute_Module_Route_NotDeleted")
                .table(function_route::Entity)
                .col(function_route::Column::Module)
                .col(function_route::Column::Route)
                .unique()
                .and_where(
                    Expr::col((function_route::Entity, function_route::Column::DeleteTime))
                        .is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_UserToken_UserId_NotDeleted")
                .table(user_token::Entity)
                .col(user_token::Column::UserId)
                .and_where(
                    Expr::col((user_token::Entity, user_token::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_TaskSetting_Category_Name_NotDeleted")
                .table(task_setting::Entity)
                .col(task_setting::Column::Category)
                .col(task_setting::Column::Name)
                .and_where(
                    Expr::col((task_setting::Entity, task_setting::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_QueueTask_Name_NotDeleted")
                .table(queue_task::Entity)
                .col(queue_task::Column::Name)
                .and_where(
                    Expr::col((queue_task::Entity, queue_task::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_QueueTask_SuccessTime_NotDeleted")
                .table(queue_task::Entity)
                .col(queue_task::Column::SuccessTime)
                .and_where(
                    Expr::col((queue_task::Entity, queue_task::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_QueueTask_Status_PlanTime_NotDeleted")
                .table(queue_task::Entity)
                .col(queue_task::Column::Status)
                .col(queue_task::Column::PlanTime)
                .and_where(
                    Expr::col((queue_task::Entity, queue_task::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_QueueTask_LeaseExpireTime_NotDeleted")
                .table(queue_task::Entity)
                .col(queue_task::Column::LeaseExpireTime)
                .and_where(
                    Expr::col((queue_task::Entity, queue_task::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_File_Table_TableId_NotDeleted")
                .table(file::Entity)
                .col(file::Column::Table)
                .col(file::Column::TableId)
                .and_where(Expr::col((file::Entity, file::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_File_Path_NotDeleted")
                .table(file::Entity)
                .col(file::Column::Path)
                .and_where(Expr::col((file::Entity, file::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Log_Category_CreateTime_NotDeleted")
                .table(log::Entity)
                .col(log::Column::Category)
                .col(log::Column::CreateTime)
                .and_where(Expr::col((log::Entity, log::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Log_Project_CreateTime_NotDeleted")
                .table(log::Entity)
                .col(log::Column::Project)
                .col(log::Column::CreateTime)
                .and_where(Expr::col((log::Entity, log::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_RegionCity_ProvinceId_NotDeleted")
                .table(region_city::Entity)
                .col(region_city::Column::ProvinceId)
                .and_where(
                    Expr::col((region_city::Entity, region_city::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_RegionArea_CityId_NotDeleted")
                .table(region_area::Entity)
                .col(region_area::Column::CityId)
                .and_where(
                    Expr::col((region_area::Entity, region_area::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_RegionTown_AreaId_NotDeleted")
                .table(region_town::Entity)
                .col(region_town::Column::AreaId)
                .and_where(
                    Expr::col((region_town::Entity, region_town::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_UserInfo_UserId_NotDeleted")
                .table(user_info::Entity)
                .col(user_info::Column::UserId)
                .unique()
                .and_where(Expr::col((user_info::Entity, user_info::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Category_ParentId_NotDeleted")
                .table(category::Entity)
                .col(category::Column::ParentId)
                .and_where(Expr::col((category::Entity, category::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Article_CategoryId_NotDeleted")
                .table(article::Entity)
                .col(article::Column::CategoryId)
                .and_where(Expr::col((article::Entity, article::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Article_IsDisplay_NotDeleted")
                .table(article::Entity)
                .col(article::Column::IsDisplay)
                .and_where(Expr::col((article::Entity, article::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_DataUpdateLog_Table")
                .table(data_update_log::Entity)
                .col(data_update_log::Column::Table)
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_DataUpdateLog_TableId")
                .table(data_update_log::Entity)
                .col(data_update_log::Column::TableId)
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_DataUpdateLog_CreateTime_NotDeleted")
                .table(data_update_log::Entity)
                .col(data_update_log::Column::CreateTime)
                .and_where(
                    Expr::col((data_update_log::Entity, data_update_log::Column::DeleteTime))
                        .is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_AppSetting_Module_Key_NotDeleted")
                .table(app_setting::Entity)
                .col(app_setting::Column::Module)
                .col(app_setting::Column::Key)
                .and_where(
                    Expr::col((app_setting::Entity, app_setting::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_AppSetting_Module_GroupId_NotDeleted")
                .table(app_setting::Entity)
                .col(app_setting::Column::Module)
                .col(app_setting::Column::GroupId)
                .and_where(
                    Expr::col((app_setting::Entity, app_setting::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_LlmApp_Code_NotDeleted")
                .table(llm_app::Entity)
                .col(llm_app::Column::Code)
                .unique()
                .and_where(
                    Expr::col((llm_app::Entity, llm_app::Column::Code))
                        .is_not_null()
                        .and(Expr::col((llm_app::Entity, llm_app::Column::DeleteTime)).is_null()),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_LlmApp_Name_NotDeleted")
                .table(llm_app::Entity)
                .col(llm_app::Column::Name)
                .and_where(Expr::col((llm_app::Entity, llm_app::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_LlmApp_IsEnable_NotDeleted")
                .table(llm_app::Entity)
                .col(llm_app::Column::IsEnable)
                .and_where(Expr::col((llm_app::Entity, llm_app::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_LlmApp_LlmModelId_NotDeleted")
                .table(llm_app::Entity)
                .col(llm_app::Column::LlmModelId)
                .and_where(Expr::col((llm_app::Entity, llm_app::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_LlmModel_IsEnable_NotDeleted")
                .table(llm_model::Entity)
                .col(llm_model::Column::IsEnable)
                .and_where(Expr::col((llm_model::Entity, llm_model::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_LlmConversation_LlmAppId_NotDeleted")
                .table(llm_conversation::Entity)
                .col(llm_conversation::Column::LlmAppId)
                .and_where(
                    Expr::col((
                        llm_conversation::Entity,
                        llm_conversation::Column::DeleteTime,
                    ))
                    .is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_LlmConversation_CreateUserId_NotDeleted")
                .table(llm_conversation::Entity)
                .col(llm_conversation::Column::CreateUserId)
                .and_where(
                    Expr::col((
                        llm_conversation::Entity,
                        llm_conversation::Column::DeleteTime,
                    ))
                    .is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_LlmConversation_CreateTime_NotDeleted")
                .table(llm_conversation::Entity)
                .col(llm_conversation::Column::CreateTime)
                .and_where(
                    Expr::col((
                        llm_conversation::Entity,
                        llm_conversation::Column::DeleteTime,
                    ))
                    .is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Product_SKU_NotDeleted")
                .table(product::Entity)
                .col(product::Column::Sku)
                .unique()
                .and_where(Expr::col((product::Entity, product::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Order_OrderNo_NotDeleted")
                .table(order::Entity)
                .col(order::Column::OrderNo)
                .unique()
                .and_where(Expr::col((order::Entity, order::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_Order_CreateUserId_NotDeleted")
                .table(order::Entity)
                .col(order::Column::CreateUserId)
                .and_where(Expr::col((order::Entity, order::Column::DeleteTime)).is_null())
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_OrderDetail_OrderId_NotDeleted")
                .table(order_detail::Entity)
                .col(order_detail::Column::OrderId)
                .and_where(
                    Expr::col((order_detail::Entity, order_detail::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_OrderDetail_ProductId_NotDeleted")
                .table(order_detail::Entity)
                .col(order_detail::Column::ProductId)
                .and_where(
                    Expr::col((order_detail::Entity, order_detail::Column::DeleteTime)).is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_UserBindExternal_AppName_AppId_OpenId_NotDeleted")
                .table(user_bind_external::Entity)
                .col(user_bind_external::Column::AppName)
                .col(user_bind_external::Column::AppId)
                .col(user_bind_external::Column::OpenId)
                .unique()
                .and_where(
                    Expr::col((
                        user_bind_external::Entity,
                        user_bind_external::Column::DeleteTime,
                    ))
                    .is_null(),
                )
                .to_owned(),
        ),
        build_index(
            backend,
            Index::create()
                .if_not_exists()
                .name("IX_UserBindExternal_UserId_NotDeleted")
                .table(user_bind_external::Entity)
                .col(user_bind_external::Column::UserId)
                .and_where(
                    Expr::col((
                        user_bind_external::Entity,
                        user_bind_external::Column::DeleteTime,
                    ))
                    .is_null(),
                )
                .to_owned(),
        ),
    ]
}

/// 获取 schema 补丁语句
fn schema_patch_statements(backend: DbBackend) -> Vec<String> {
    vec![
        build_alter(
            backend,
            Table::alter()
                .table(file::Entity)
                .add_column_if_not_exists(ColumnDef::new(file::Column::TableId).big_integer())
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(file::Entity)
                .add_column_if_not_exists(ColumnDef::new(file::Column::Sign).text())
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(file::Entity)
                .modify_column(
                    ColumnDef::new(file::Column::Sort)
                        .integer()
                        .not_null()
                        .default(0),
                )
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(queue_task::Entity)
                .add_column_if_not_exists(
                    ColumnDef::new(queue_task::Column::Status)
                        .integer()
                        .not_null()
                        .default(0),
                )
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(queue_task::Entity)
                .add_column_if_not_exists(ColumnDef::new(queue_task::Column::WorkerId).text())
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(queue_task::Entity)
                .add_column_if_not_exists(
                    ColumnDef::new(queue_task::Column::LeaseExpireTime).timestamp_with_time_zone(),
                )
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(queue_task::Entity)
                .modify_column(
                    ColumnDef::new(queue_task::Column::Status)
                        .integer()
                        .not_null()
                        .default(0),
                )
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(llm_app::Entity)
                .add_column_if_not_exists(
                    ColumnDef::new(llm_app::Column::LlmModelId)
                        .big_integer()
                        .not_null(),
                )
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(llm_app::Entity)
                .add_column_if_not_exists(ColumnDef::new(llm_app::Column::Code).text())
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(llm_app::Entity)
                .add_column_if_not_exists(
                    ColumnDef::new(llm_app::Column::SystemPromptTemplate).text(),
                )
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(llm_app::Entity)
                .add_column_if_not_exists(ColumnDef::new(llm_app::Column::PromptTemplate).text())
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(llm_app::Entity)
                .add_column_if_not_exists(ColumnDef::new(llm_app::Column::ExtraBodyJson).text())
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(llm_app::Entity)
                .add_column_if_not_exists(
                    ColumnDef::new(llm_app::Column::IsEnable)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(llm_app::Entity)
                .add_column_if_not_exists(
                    ColumnDef::new(llm_app::Column::UpdateTime).timestamp_with_time_zone(),
                )
                .to_owned(),
        ),
        build_alter(
            backend,
            Table::alter()
                .table(llm_app::Entity)
                .add_column_if_not_exists(
                    ColumnDef::new(llm_app::Column::UpdateUserId).big_integer(),
                )
                .to_owned(),
        ),
    ]
}

/// 追加实体建表语句
fn append_entity_table<E>(
    statements: &mut Vec<String>,
    backend: DbBackend,
    schema: &Schema,
    entity: E,
) where
    E: EntityTrait,
{
    let mut statement = schema.create_table_from_entity(entity);
    statement.if_not_exists();
    statements.push(backend.build(&statement).to_string());
}

/// 追加缺失实体建表语句
fn append_missing_entity_table<E>(statements: &mut Vec<String>, backend: DbBackend, entity: E)
where
    E: EntityTrait,
{
    let schema = Schema::new(backend);
    append_entity_table(statements, backend, &schema, entity);
}

/// 构建索引 SQL
fn build_index(backend: DbBackend, statement: sea_orm::sea_query::IndexCreateStatement) -> String {
    backend.build(&statement).to_string()
}

/// 构建表变更 SQL
fn build_alter(backend: DbBackend, statement: TableAlterStatement) -> String {
    backend.build(&statement).to_string()
}
