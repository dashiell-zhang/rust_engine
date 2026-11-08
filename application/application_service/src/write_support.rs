#![allow(clippy::too_many_arguments)]

use repository_core::versioned::VersionedActiveModel;

use chrono::Utc;
use repository::data_update_log;
use sea_orm::{
    ActiveValue, ActiveValue::Set, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    TransactionTrait, entity::prelude::DateTimeWithTimeZone,
};
use serde_json::Value;

use application_interface::{
    context::{AppContext, CurrentUser},
    error::AppError,
};
use common::identifier_generator::IdentifierGenerator;

pub use application_model::shared::models::{page_offset, validate_page_request};
pub use common::string_helper::normalize_optional_text;

/// 创建操作审计字段
pub struct CreateAuditFields {
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 删除时间
    pub delete_time: Option<DateTimeWithTimeZone>,
    /// 创建人
    pub create_user_id: Option<i64>,
    /// 删除人
    pub delete_user_id: Option<i64>,
}

/// 创建包含更新字段的审计字段
pub struct CreateFullAuditFields {
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 删除时间
    pub delete_time: Option<DateTimeWithTimeZone>,
    /// 更新时间
    pub update_time: Option<DateTimeWithTimeZone>,
    /// 创建人
    pub create_user_id: Option<i64>,
    /// 更新人
    pub update_user_id: Option<i64>,
    /// 删除人
    pub delete_user_id: Option<i64>,
}

/// 创建系统级审计字段
pub struct SystemCreateAuditFields {
    /// 创建时间
    pub create_time: DateTimeWithTimeZone,
    /// 删除时间
    pub delete_time: Option<DateTimeWithTimeZone>,
}

/// 构建创建操作审计字段
pub fn build_create_audit_fields(current_user: &CurrentUser) -> CreateAuditFields {
    CreateAuditFields {
        create_time: Utc::now().into(),
        delete_time: None,
        create_user_id: Some(current_user.user_id),
        delete_user_id: None,
    }
}

/// 构建完整创建审计字段
pub fn build_create_full_audit_fields(current_user: &CurrentUser) -> CreateFullAuditFields {
    CreateFullAuditFields {
        create_time: Utc::now().into(),
        delete_time: None,
        update_time: None,
        create_user_id: Some(current_user.user_id),
        update_user_id: None,
        delete_user_id: None,
    }
}

/// 构建系统级创建审计字段
pub fn build_system_create_audit_fields() -> SystemCreateAuditFields {
    SystemCreateAuditFields {
        create_time: Utc::now().into(),
        delete_time: None,
    }
}

/// 应用更新用户审计字段
pub fn apply_update_user_audit_field(
    update_user_id: &mut ActiveValue<Option<i64>>,
    current_user: &CurrentUser,
) {
    *update_user_id = Set(Some(current_user.user_id));
}

/// 应用软删除审计字段
pub fn apply_soft_delete_audit_fields(
    delete_time: &mut ActiveValue<Option<DateTimeWithTimeZone>>,
    delete_user_id: &mut ActiveValue<Option<i64>>,
    current_user: &CurrentUser,
) -> DateTimeWithTimeZone {
    let now: DateTimeWithTimeZone = Utc::now().into();
    *delete_time = Set(Some(now));
    *delete_user_id = Set(Some(current_user.user_id));
    now
}

/// 创建写操作审计日志
/// 日志写入失败时不影响主业务成功
pub async fn write_data_update_log(
    context: AppContext<'_>,
    table_name: &str,
    table_id: i64,
    operation: &str,
    before: Option<Value>,
    after: Option<Value>,
    current_user: Option<&CurrentUser>,
) -> Result<(), AppError> {
    write_data_update_log_with_connection(
        context.db,
        context.id_generator,
        table_name,
        table_id,
        operation,
        before,
        after,
        current_user,
    )
    .await
}

/// 使用指定连接创建写操作审计日志
/// 适用于事务内写库场景
/// 日志写入失败时不影响主业务成功
pub async fn write_data_update_log_with_connection<C>(
    db: &C,
    id_generator: &IdentifierGenerator,
    table_name: &str,
    table_id: i64,
    operation: &str,
    before: Option<Value>,
    after: Option<Value>,
    current_user: Option<&CurrentUser>,
) -> Result<(), AppError>
where
    C: ConnectionTrait + sea_orm::TransactionTrait,
{
    let log_id = id_generator
        .get_id()
        .await
        .map_err(|error| AppError::internal(format!("生成数据变更日志主键失败: {error}")))?;
    let content = serde_json::json!({
        "operation": operation,
        "before": before,
        "after": after,
    });
    let content = serde_json::to_string(&content)
        .map_err(|error| AppError::internal(format!("序列化数据变更日志失败: {error}")))?;

    data_update_log::ActiveModel {
        row_version: Default::default(),
        id: Set(log_id),
        table: Set(table_name.to_string()),
        table_id: Set(table_id),
        content: Set(content),
        action_user_id: Set(current_user.map(|item| item.user_id)),
        ip_address: Set(None),
        device_mark: Set(None),
        create_time: Set(Utc::now().into()),
        delete_time: Set(None),
    }
    .insert_versioned(db)
    .await
    .ok();

    Ok(())
}

/// 开启应用层事务
/// 用于把同一业务动作下的多次写库收敛到同一个提交单元
pub async fn begin_transaction(db: &DatabaseConnection) -> Result<DatabaseTransaction, AppError> {
    db.begin()
        .await
        .map_err(|error| AppError::internal(format!("开启事务失败: {error}")))
}
