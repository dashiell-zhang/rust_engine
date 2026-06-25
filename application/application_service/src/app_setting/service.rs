use chrono::Utc;
use distributed_lock::LockError;
use repository::app_setting;
use repository::extensions::SoftDeleteQueryExt;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, QuerySelect,
};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

use application_interface::{
    context::{AppSettingContext, CurrentUser},
    error::AppError,
};

use crate::write_support::{begin_transaction, write_data_update_log_with_connection};

use super::models::{
    AppSettingDto, DeleteAppSettingQuery, GetAppSettingListQuery, GetAppSettingQuery,
    SetAppSettingDto,
};

/// 应用配置管理服务
pub struct AppSettingService<'a> {
    /// 应用层上下文
    context: AppSettingContext<'a>,
}

impl<'a> AppSettingService<'a> {
    /// 创建应用配置管理服务
    pub fn new(context: AppSettingContext<'a>) -> Self {
        Self { context }
    }

    /// 获取指定模块下的所有配置项
    pub async fn get_setting_list(
        &self,
        query: GetAppSettingListQuery,
    ) -> Result<Vec<AppSettingDto>, AppError> {
        // 先做参数校验 避免把空模块名带入查询条件
        if query.module.trim().is_empty() {
            return Err(AppError::bad_request("配置模块不可以为空"));
        }

        let models = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(query.module.trim()))
            .not_deleted(app_setting::Column::DeleteTime)
            .order_by_asc(app_setting::Column::Key)
            .all(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        Ok(models.into_iter().map(build_app_setting_dto).collect())
    }

    /// 获取指定模块与键对应的单个配置项
    pub async fn get_setting(
        &self,
        query: GetAppSettingQuery,
    ) -> Result<Option<AppSettingDto>, AppError> {
        // 单项查询要求模块和键都明确提供
        if query.module.trim().is_empty() {
            return Err(AppError::bad_request("配置模块不可以为空"));
        }

        if query.key.trim().is_empty() {
            return Err(AppError::bad_request("配置键不可以为空"));
        }

        let model = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(query.module.trim()))
            .filter(app_setting::Column::Key.eq(query.key.trim()))
            .not_deleted(app_setting::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        Ok(model.map(build_app_setting_dto))
    }

    /// 获取指定字典分组下的键值对列表
    pub async fn get_value_list(&self, group_id: i64) -> Result<HashMap<String, String>, AppError> {
        // 字典数据统一存放在 Dictionary 模块下 由 group_id 区分不同字典组
        let list = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq("Dictionary"))
            .filter(app_setting::Column::GroupId.eq(group_id))
            .not_deleted(app_setting::Column::DeleteTime)
            .order_by_asc(app_setting::Column::Key)
            .all(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        // 直接转换为键值对字典 供前端下拉选项等场景直接消费
        let mut result = HashMap::with_capacity(list.len());
        for item in list {
            result.insert(item.key, item.value);
        }

        Ok(result)
    }

    /// 写入配置项，存在则更新值，不存在则创建
    pub async fn set_setting(
        &self,
        dto: SetAppSettingDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        // 配置项以 module + key 作为业务唯一键
        if dto.module.trim().is_empty() {
            return Err(AppError::bad_request("配置模块不可以为空"));
        }

        if dto.key.trim().is_empty() {
            return Err(AppError::bad_request("配置键不可以为空"));
        }

        let module = dto.module.trim().to_string();
        let key = dto.key.trim().to_string();
        let value = dto.value;
        let group_id = dto.group_id;
        let lock_key = build_setting_lock_key(&module, &key);
        let _lock_guard = self
            .context
            .setting_lock
            .lock(&lock_key, Duration::from_secs(10), 1)
            .await
            .map_err(|error| match error {
                LockError::Timeout { .. } => AppError::conflict("配置项正在保存，请稍后重试"),
                _ => AppError::from(error),
            })?;

        let tx = begin_transaction(self.context.app.db).await?;
        let existing = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(module.as_str()))
            .filter(app_setting::Column::Key.eq(key.as_str()))
            .not_deleted(app_setting::Column::DeleteTime)
            .order_by_desc(app_setting::Column::Id)
            .one(&tx)
            .await
            .map_err(to_app_error)?;

        match existing {
            Some(model) => {
                // 更新时保留原记录主键，仅覆盖值和可选分组。
                let before_log = json!({
                    "id": model.id,
                    "module": model.module,
                    "key": model.key,
                    "value": model.value,
                    "group_id": model.group_id,
                });
                let mut active = model.into_active_model();
                active.module = Set(module.clone());
                active.key = Set(key.clone());
                active.value = Set(value.clone());
                if let Some(group_id) = group_id {
                    active.group_id = Set(Some(group_id));
                }
                let updated = active
                    .update(&tx)
                    .await
                    .map_err(|error| AppError::internal(format!("更新配置项失败: {error}")))?;

                write_data_update_log_with_connection(
                    &tx,
                    self.context.app.id_generator,
                    "AppSetting",
                    updated.id,
                    "update",
                    Some(before_log),
                    Some(json!({
                        "id": updated.id,
                        "module": updated.module,
                        "key": updated.key,
                        "value": updated.value,
                        "group_id": updated.group_id,
                    })),
                    Some(current_user),
                )
                .await?;
            }
            None => {
                // 新建时生成主键并记录完整快照 便于后续审计追踪
                let setting_id = self
                    .context
                    .app
                    .id_generator
                    .get_id()
                    .await
                    .map_err(|error| {
                        AppError::internal(format!("生成应用配置主键失败: {error}"))
                    })?;
                let after_log = json!({
                    "id": setting_id,
                    "module": module,
                    "key": key,
                    "value": value,
                    "group_id": group_id,
                });

                app_setting::ActiveModel {
                    id: Set(setting_id),
                    module: Set(module),
                    group_id: Set(group_id),
                    key: Set(key),
                    value: Set(value),
                    create_time: Set(Utc::now().into()),
                    delete_time: Set(None),
                }
                .insert(&tx)
                .await
                .map_err(|error| AppError::internal(format!("创建配置项失败: {error}")))?;

                write_data_update_log_with_connection(
                    &tx,
                    self.context.app.id_generator,
                    "AppSetting",
                    setting_id,
                    "create",
                    None,
                    Some(after_log),
                    Some(current_user),
                )
                .await?;
            }
        }

        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 软删除指定模块与键对应的配置项
    pub async fn delete_setting(
        &self,
        query: DeleteAppSettingQuery,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        // 删除接口保持幂等 目标不存在时直接返回成功
        if query.module.trim().is_empty() {
            return Err(AppError::bad_request("配置模块不可以为空"));
        }

        if query.key.trim().is_empty() {
            return Err(AppError::bad_request("配置键不可以为空"));
        }

        let model = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(query.module.trim()))
            .filter(app_setting::Column::Key.eq(query.key.trim()))
            .not_deleted(app_setting::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(true);
        };

        let tx = begin_transaction(self.context.app.db).await?;
        let before_log = json!({
            "id": model.id,
            "module": model.module,
            "key": model.key,
            "value": model.value,
        });
        let setting_id = model.id;
        let mut active = model.into_active_model();
        active.delete_time = Set(Some(Utc::now().into()));
        active
            .update(&tx)
            .await
            .map_err(|error| AppError::internal(format!("删除配置项失败: {error}")))?;

        write_data_update_log_with_connection(
            &tx,
            self.context.app.id_generator,
            "AppSetting",
            setting_id,
            "delete",
            Some(before_log),
            Some(json!({ "id": setting_id })),
            Some(current_user),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 获取所有已使用的配置模块名列表
    pub async fn get_module_list(&self) -> Result<Vec<String>, AppError> {
        let modules = app_setting::Entity::find()
            .select_only()
            .column(app_setting::Column::Module)
            .not_deleted(app_setting::Column::DeleteTime)
            .into_tuple::<String>()
            .all(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        // sea-orm into_tuple 不支持直接 DISTINCT，在内存中排序后去重等价
        let mut unique = modules;
        unique.sort();
        unique.dedup();

        Ok(unique)
    }
}

/// 构建配置项写入锁 key
fn build_setting_lock_key(module: &str, key: &str) -> String {
    format!("app_setting:set:{}:{}:{}", module.len(), module, key)
}

/// 组装配置项返回模型
fn build_app_setting_dto(model: app_setting::Model) -> AppSettingDto {
    // 统一在服务层完成数据库模型到接口模型的转换
    AppSettingDto {
        id: model.id.to_string(),
        module: model.module,
        group_id: model.group_id,
        key: model.key,
        value: model.value,
        create_time: model.create_time.to_rfc3339(),
    }
}

/// 转换数据库异常
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}
