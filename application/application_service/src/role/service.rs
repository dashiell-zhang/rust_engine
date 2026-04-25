use async_recursion::async_recursion;
use chrono::Utc;
use common::string_helper::trim_prefix_name;
use repository::extensions::SoftDeleteQueryExt;
use repository::{function, function_authorize, role, user_role};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde_json::json;
use std::collections::HashSet;

use application_interface::{
    context::{CurrentUser, RoleContext},
    error::AppError,
};

use crate::write_support::{
    apply_soft_delete_audit_fields, begin_transaction, build_create_full_audit_fields,
    build_system_create_audit_fields, normalize_page, validate_page_request,
    write_data_update_log_with_connection,
};

use super::models::{
    EditRoleDto, RoleDto, RoleFunctionDto, RolePageListDto, RolePageRequest, RoleSelectDto,
    SetRoleFunctionDto,
};

/// 模块类型
const FUNCTION_TYPE_MODULE: i32 = 1;

/// 功能类型
const FUNCTION_TYPE_FUNCTION: i32 = 2;

/// 角色模块应用服务
pub struct RoleService<'a> {
    /// 角色模块上下文
    context: RoleContext<'a>,
}

impl<'a> RoleService<'a> {
    /// 创建角色模块应用服务
    pub fn new(context: RoleContext<'a>) -> Self {
        Self { context }
    }

    /// 获取角色列表
    pub async fn get_role_list(
        &self,
        params: RolePageRequest,
    ) -> Result<RolePageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let total = role::Entity::find()
            .not_deleted(role::Column::DeleteTime)
            .count(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        if total == 0 {
            return Ok(RolePageListDto {
                total: 0,
                list: vec![],
            });
        }

        let page = normalize_page(params.page);
        let page_size = params.page_size;
        let skip = (page - 1) * page_size;

        let roles = role::Entity::find()
            .not_deleted(role::Column::DeleteTime)
            .order_by_desc(role::Column::Id)
            .offset(skip)
            .limit(page_size)
            .all(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        Ok(RolePageListDto {
            total,
            list: roles.into_iter().map(build_role_dto).collect(),
        })
    }

    /// 获取单个角色
    pub async fn get_role(&self, role_id: i64) -> Result<Option<RoleDto>, AppError> {
        let role_model = role::Entity::find()
            .filter(role::Column::Id.eq(role_id))
            .not_deleted(role::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        Ok(role_model.map(build_role_dto))
    }

    /// 校验编辑角色请求
    pub fn validate_edit_role(dto: &EditRoleDto) -> Result<(), AppError> {
        if dto.code.trim().is_empty() {
            return Err(AppError::new("角色编码不可以空"));
        }

        if dto.name.trim().is_empty() {
            return Err(AppError::new("名称不可以空"));
        }

        Ok(())
    }

    /// 创建角色
    pub async fn create_role(&self, dto: EditRoleDto) -> Result<String, AppError> {
        Self::validate_edit_role(&dto)?;

        let lock_guard = self
            .context
            .role_code_lock
            .try_lock(
                &format!("roleCode{}", dto.code),
                std::time::Duration::from_secs(5),
                1,
            )
            .await?;
        if lock_guard.is_none() {
            return Err(AppError::conflict("角色编码已被占用"));
        }

        let tx = begin_transaction(self.context.app.db).await?;

        if role::Entity::find()
            .filter(role::Column::Code.eq(dto.code.as_str()))
            .not_deleted(role::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?
            .is_some()
        {
            return Err(AppError::conflict("角色编码已被占用"));
        }

        let role_id = self
            .context
            .app
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成角色主键失败: {error}")))?;
        let audit_after = json!({
            "id": role_id,
            "code": dto.code,
            "name": dto.name,
            "remark": dto.remark,
        });
        let audit_fields = build_system_create_audit_fields();

        let role_model = role::ActiveModel {
            id: Set(role_id),
            code: Set(dto.code),
            name: Set(dto.name),
            remark: Set(dto.remark),
            create_time: Set(audit_fields.create_time),
            delete_time: Set(audit_fields.delete_time),
        };

        role_model
            .insert(&tx)
            .await
            .map_err(|error| AppError::internal(format!("创建角色失败: {error}")))?;

        write_data_update_log_with_connection(
            &tx,
            self.context.app.id_generator,
            "Role",
            role_id,
            "create",
            None,
            Some(audit_after),
            None,
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(role_id.to_string())
    }

    /// 更新角色
    pub async fn update_role(&self, role_id: i64, dto: EditRoleDto) -> Result<bool, AppError> {
        Self::validate_edit_role(&dto)?;

        let lock_guard = self
            .context
            .role_code_lock
            .try_lock(
                &format!("roleCode{}", dto.code),
                std::time::Duration::from_secs(5),
                1,
            )
            .await?;
        if lock_guard.is_none() {
            return Err(AppError::conflict("角色编码已被占用"));
        }

        let tx = begin_transaction(self.context.app.db).await?;

        if role::Entity::find()
            .filter(role::Column::Code.eq(dto.code.as_str()))
            .filter(role::Column::Id.ne(role_id))
            .not_deleted(role::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?
            .is_some()
        {
            return Err(AppError::conflict("角色编码已被占用"));
        }

        let role_model = role::Entity::find()
            .filter(role::Column::Id.eq(role_id))
            .not_deleted(role::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?;

        let Some(role_model) = role_model else {
            return Err(AppError::bad_request("角色不存在无法编辑"));
        };

        let before_log = json!({
            "id": role_model.id,
            "code": role_model.code,
            "name": role_model.name,
            "remark": role_model.remark,
        });
        let mut active = role_model.into_active_model();
        active.code = Set(dto.code);
        active.name = Set(dto.name);
        active.remark = Set(dto.remark);
        let updated_model = active
            .update(&tx)
            .await
            .map_err(|error| AppError::internal(format!("更新角色失败: {error}")))?;

        write_data_update_log_with_connection(
            &tx,
            self.context.app.id_generator,
            "Role",
            role_id,
            "update",
            Some(before_log),
            Some(json!({
                "id": updated_model.id,
                "code": updated_model.code,
                "name": updated_model.name,
                "remark": updated_model.remark,
            })),
            None,
        )
        .await?;

        let affected_user_ids = load_role_user_ids(&tx, role_id)
            .await
            .map_err(to_app_error)?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;
        invalidate_user_authorize_cache_list(self.context.cache, &affected_user_ids).await;

        Ok(true)
    }

    /// 删除角色
    pub async fn delete_role(&self, role_id: i64) -> Result<bool, AppError> {
        let tx = begin_transaction(self.context.app.db).await?;

        if user_role::Entity::find()
            .filter(user_role::Column::RoleId.eq(role_id))
            .not_deleted(user_role::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?
            .is_some()
        {
            return Err(AppError::conflict("当前角色下存在人员信息，无法删除！"));
        }

        let role_model = role::Entity::find()
            .filter(role::Column::Id.eq(role_id))
            .not_deleted(role::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?;

        if let Some(role_model) = role_model {
            let delete_time = Utc::now();
            let before_log = json!({
                "id": role_model.id,
                "code": role_model.code,
                "name": role_model.name,
                "remark": role_model.remark,
            });
            let mut active = role_model.into_active_model();
            active.delete_time = Set(Some(delete_time.into()));
            let _ = active
                .update(&tx)
                .await
                .map_err(|error| AppError::internal(format!("删除角色失败: {error}")))?;

            write_data_update_log_with_connection(
                &tx,
                self.context.app.id_generator,
                "Role",
                role_id,
                "delete",
                Some(before_log),
                Some(json!({
                    "id": role_id,
                    "delete_time": delete_time.to_rfc3339(),
                })),
                None,
            )
            .await?;
        }

        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 获取角色功能树
    pub async fn get_role_function(&self, role_id: i64) -> Result<Vec<RoleFunctionDto>, AppError> {
        load_role_function_tree(self.context.app.db, role_id, None, None)
            .await
            .map_err(to_app_error)
    }

    /// 设置角色功能
    pub async fn set_role_function(
        &self,
        dto: SetRoleFunctionDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let tx = begin_transaction(self.context.app.db).await?;
        let existing = function_authorize::Entity::find()
            .filter(function_authorize::Column::RoleId.eq(dto.role_id))
            .filter(function_authorize::Column::FunctionId.eq(dto.function_id))
            .not_deleted(function_authorize::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?;

        if dto.is_check {
            if existing.is_none() {
                let authorize_id =
                    self.context
                        .app
                        .id_generator
                        .get_id()
                        .await
                        .map_err(|error| {
                            AppError::internal(format!("生成角色功能授权主键失败: {error}"))
                        })?;
                let audit_fields = build_create_full_audit_fields(current_user);

                let active = function_authorize::ActiveModel {
                    id: Set(authorize_id),
                    function_id: Set(dto.function_id),
                    role_id: Set(Some(dto.role_id)),
                    user_id: Set(None),
                    create_time: Set(audit_fields.create_time),
                    delete_time: Set(audit_fields.delete_time),
                    update_time: Set(audit_fields.update_time),
                    create_user_id: Set(audit_fields.create_user_id),
                    update_user_id: Set(audit_fields.update_user_id),
                    delete_user_id: Set(audit_fields.delete_user_id),
                };

                active
                    .insert(&tx)
                    .await
                    .map_err(|error| AppError::internal(format!("设置角色功能失败: {error}")))?;

                write_data_update_log_with_connection(
                    &tx,
                    self.context.app.id_generator,
                    "FunctionAuthorize",
                    authorize_id,
                    "create",
                    None,
                    Some(json!({
                        "id": authorize_id,
                        "role_id": dto.role_id,
                        "function_id": dto.function_id,
                    })),
                    Some(current_user),
                )
                .await?;
            }
        } else if let Some(model) = existing {
            let authorize_id = model.id;
            let before_log = json!({
                "id": model.id,
                "role_id": model.role_id,
                "function_id": model.function_id,
                "user_id": model.user_id,
            });
            let mut active = model.into_active_model();
            apply_soft_delete_audit_fields(
                &mut active.delete_time,
                &mut active.delete_user_id,
                current_user,
            );
            let _ = active
                .update(&tx)
                .await
                .map_err(|error| AppError::internal(format!("取消角色功能失败: {error}")))?;

            write_data_update_log_with_connection(
                &tx,
                self.context.app.id_generator,
                "FunctionAuthorize",
                authorize_id,
                "delete",
                Some(before_log),
                Some(json!({
                    "role_id": dto.role_id,
                    "function_id": dto.function_id,
                })),
                Some(current_user),
            )
            .await?;
        }

        let affected_user_ids = load_role_user_ids(&tx, dto.role_id)
            .await
            .map_err(to_app_error)?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;
        invalidate_user_authorize_cache_list(self.context.cache, &affected_user_ids).await;

        Ok(true)
    }

    /// 获取角色下拉列表
    pub async fn get_role_select(&self) -> Result<Vec<RoleSelectDto>, AppError> {
        let roles = role::Entity::find()
            .not_deleted(role::Column::DeleteTime)
            .order_by_desc(role::Column::Id)
            .all(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        Ok(roles
            .into_iter()
            .map(|item| RoleSelectDto {
                id: item.id.to_string(),
                name: item.name,
            })
            .collect())
    }
}

/// 将角色实体组装为角色返回模型
fn build_role_dto(model: role::Model) -> RoleDto {
    RoleDto {
        id: model.id.to_string(),
        code: model.code,
        name: model.name,
        remark: model.remark,
        create_time: model.create_time.to_rfc3339(),
    }
}

/// 递归加载角色功能树
#[async_recursion]
async fn load_role_function_tree(
    db: &DatabaseConnection,
    role_id: i64,
    parent_id: Option<i64>,
    parent_name: Option<String>,
) -> Result<Vec<RoleFunctionDto>, DbErr> {
    let module_query = match parent_id {
        Some(parent_id) => {
            function::Entity::find().filter(function::Column::ParentId.eq(parent_id))
        }
        None => function::Entity::find().filter(function::Column::ParentId.is_null()),
    };

    let module_list = module_query
        .filter(function::Column::Type.eq(FUNCTION_TYPE_MODULE))
        .not_deleted(function::Column::DeleteTime)
        .order_by_asc(function::Column::Id)
        .all(db)
        .await?;

    if module_list.is_empty() {
        return Ok(vec![]);
    }

    let module_ids = module_list.iter().map(|item| item.id).collect::<Vec<_>>();

    let child_functions = function::Entity::find()
        .filter(function::Column::ParentId.is_in(module_ids.clone()))
        .filter(function::Column::Type.eq(FUNCTION_TYPE_FUNCTION))
        .not_deleted(function::Column::DeleteTime)
        .order_by_asc(function::Column::Id)
        .all(db)
        .await?;

    let all_function_ids = module_ids
        .iter()
        .copied()
        .chain(child_functions.iter().map(|item| item.id))
        .collect::<Vec<_>>();

    let checked_ids = function_authorize::Entity::find()
        .filter(function_authorize::Column::RoleId.eq(role_id))
        .not_deleted(function_authorize::Column::DeleteTime)
        .filter(function_authorize::Column::FunctionId.is_in(all_function_ids))
        .all(db)
        .await?
        .into_iter()
        .map(|item| item.function_id)
        .collect::<HashSet<_>>();

    let mut result = Vec::with_capacity(module_list.len());

    for module in module_list {
        let module_name = trim_prefix_name(&module.name, parent_name.as_deref());
        let children =
            load_role_function_tree(db, role_id, Some(module.id), Some(module.name.clone()))
                .await?;
        let functions = child_functions
            .iter()
            .filter(|item| item.parent_id == Some(module.id))
            .map(|item| RoleFunctionDto {
                id: item.id.to_string(),
                name: trim_prefix_name(&item.name, Some(&module.name)),
                is_check: checked_ids.contains(&item.id),
                sign: item.sign.clone(),
                child_list: vec![],
                function_list: vec![],
            })
            .collect::<Vec<_>>();

        result.push(RoleFunctionDto {
            id: module.id.to_string(),
            name: module_name,
            is_check: checked_ids.contains(&module.id),
            sign: module.sign,
            child_list: children,
            function_list: functions,
        });
    }

    Ok(result)
}

/// 转换数据库异常
fn to_app_error(error: DbErr) -> AppError {
    AppError::internal(error.to_string())
}

/// 加载角色下的用户 ID 列表
async fn load_role_user_ids<C>(db: &C, role_id: i64) -> Result<Vec<i64>, DbErr>
where
    C: ConnectionTrait,
{
    user_role::Entity::find()
        .filter(user_role::Column::RoleId.eq(role_id))
        .not_deleted(user_role::Column::DeleteTime)
        .all(db)
        .await
        .map(|items| items.into_iter().map(|item| item.user_id).collect())
}

/// 批量使用户授权缓存失效
async fn invalidate_user_authorize_cache_list(
    cache: Option<&dyn application_interface::cache::DistributedCache>,
    user_ids: &[i64],
) {
    for user_id in user_ids {
        crate::authorize::service::invalidate_user_authorize_cache(cache, *user_id).await;
    }
}
