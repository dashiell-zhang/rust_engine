use async_recursion::async_recursion;
use chrono::Utc;
use common::string_helper::trim_prefix_name;
use repository::extensions::SoftDeleteQueryExt;
use repository::{function, function_authorize, role, user_role};
use repository_core::pagination::SignedPage;
use repository_core::versioned::VersionedActiveModel;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, QueryOrder, Set,
};
use std::collections::HashSet;

use application_interface::error::AppError;
use application_runtime::context::{CurrentUser, RoleContext};

use crate::write_support::{build_system_create_audit_fields, page_offset, validate_page_request};

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
        let total = repository_core::pagination::count_i32(total).map_err(to_app_error)?;

        if total == 0 {
            return Ok(RolePageListDto {
                total: 0,
                list: None,
            });
        }

        let skip = page_offset(params.page, params.page_size);

        let roles = role::Entity::find()
            .not_deleted(role::Column::DeleteTime)
            .order_by_desc(role::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.context.app.db)
            .await
            .map_err(to_app_error)?;

        Ok(RolePageListDto {
            total,
            list: Some(roles.into_iter().map(build_role_dto).collect()),
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

    /// 创建角色；编码锁与查重沿用源服务，不额外写变更日志。
    pub async fn create_role(&self, dto: EditRoleDto) -> Result<String, AppError> {
        Self::validate_edit_role(&dto)?;
        let guard = self
            .context
            .role_code_lock
            .try_lock(
                &format!("roleCode{}", dto.code),
                std::time::Duration::ZERO,
                1,
            )
            .await?
            .ok_or_else(|| AppError::bad_request("角色编码已被占用"))?;
        let result = async {
            if role::Entity::find()
                .filter(role::Column::Code.eq(&dto.code))
                .not_deleted(role::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(to_app_error)?
                .is_some()
            {
                return Err(AppError::bad_request("角色编码已被占用"));
            }
            let role_id = self
                .context
                .app
                .id_generator
                .get_id()
                .await
                .map_err(|error| AppError::internal(format!("生成角色主键失败: {error}")))?;
            let audit = build_system_create_audit_fields();
            role::ActiveModel {
                row_version: Default::default(),
                id: Set(role_id),
                code: Set(dto.code),
                name: Set(dto.name),
                remark: Set(dto.remark),
                create_time: Set(audit.create_time),
                delete_time: Set(None),
            }
            .insert_versioned(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("创建角色失败: {error}")))?;
            Ok(role_id.to_string())
        }
        .await;
        guard.release().await;
        result
    }

    /// 更新角色；先检查编码冲突，再判断目标是否存在。
    pub async fn update_role(&self, role_id: i64, dto: EditRoleDto) -> Result<bool, AppError> {
        Self::validate_edit_role(&dto)?;
        let guard = self
            .context
            .role_code_lock
            .try_lock(
                &format!("roleCode{}", dto.code),
                std::time::Duration::ZERO,
                1,
            )
            .await?
            .ok_or_else(|| AppError::bad_request("角色编码已被占用"))?;
        let result = async {
            if role::Entity::find()
                .filter(role::Column::Code.eq(&dto.code))
                .filter(role::Column::Id.ne(role_id))
                .not_deleted(role::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(to_app_error)?
                .is_some()
            {
                return Err(AppError::bad_request("角色编码已被占用"));
            }
            let model = role::Entity::find_by_id(role_id)
                .not_deleted(role::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(to_app_error)?
                .ok_or_else(|| AppError::bad_request("角色不存在无法编辑"))?;
            // EF 对相同值赋值不会产生 UPDATE，保留原 xmin。
            if model.code != dto.code || model.name != dto.name || model.remark != dto.remark {
                let mut active = model.into_active_model();
                active.code = Set(dto.code);
                active.name = Set(dto.name);
                active.remark = Set(dto.remark);
                active
                    .update_versioned(self.context.app.db)
                    .await
                    .map_err(|error| AppError::internal(format!("更新角色失败: {error}")))?;
            }
            Ok(true)
        }
        .await;
        guard.release().await;
        result
    }

    /// 删除角色；只软删除角色，不清理其授权或其他关联。
    pub async fn delete_role(&self, role_id: i64) -> Result<bool, AppError> {
        if user_role::Entity::find()
            .filter(user_role::Column::RoleId.eq(role_id))
            .not_deleted(user_role::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(to_app_error)?
            .is_some()
        {
            return Err(AppError::bad_request("当前角色下存在人员信息，无法删除！"));
        }
        if let Some(model) = role::Entity::find_by_id(role_id)
            .not_deleted(role::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(to_app_error)?
        {
            let mut active = model.into_active_model();
            active.delete_time = Set(Some(Utc::now().into()));
            active
                .update_versioned(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("删除角色失败: {error}")))?;
        }
        Ok(true)
    }

    /// 获取角色功能树
    pub async fn get_role_function(&self, role_id: i64) -> Result<Vec<RoleFunctionDto>, AppError> {
        load_role_function_tree(self.context.app.db, role_id, None, None)
            .await
            .map_err(to_app_error)
    }

    /// 设置角色功能；每次只修改查询到的第一条有效关联。
    pub async fn set_role_function(
        &self,
        dto: SetRoleFunctionDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let existing = function_authorize::Entity::find()
            .filter(function_authorize::Column::RoleId.eq(dto.role_id))
            .filter(function_authorize::Column::FunctionId.eq(dto.function_id))
            .not_deleted(function_authorize::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(to_app_error)?;
        if dto.is_check {
            if existing.is_none() {
                let id = self
                    .context
                    .app
                    .id_generator
                    .get_id()
                    .await
                    .map_err(|error| {
                        AppError::internal(format!("生成角色功能授权主键失败: {error}"))
                    })?;
                function_authorize::ActiveModel {
                    row_version: Default::default(),
                    id: Set(id),
                    function_id: Set(dto.function_id),
                    role_id: Set(Some(dto.role_id)),
                    user_id: Set(None),
                    create_time: Set(Utc::now().into()),
                    delete_time: Set(None),
                    update_time: Set(None),
                    create_user_id: Set(Some(current_user.user_id)),
                    update_user_id: Set(None),
                    delete_user_id: Set(None),
                }
                .insert_versioned(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("设置角色功能失败: {error}")))?;
            }
        } else if let Some(model) = existing {
            let mut active = model.into_active_model();
            active.delete_time = Set(Some(Utc::now().into()));
            active.delete_user_id = Set(Some(current_user.user_id));
            active
                .update_versioned(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("取消角色功能失败: {error}")))?;
        }
        Ok(true)
    }

    /// 获取角色下拉列表
    pub async fn get_role_select(&self) -> Result<Vec<RoleSelectDto>, AppError> {
        let roles = role::Entity::find()
            .not_deleted(role::Column::DeleteTime)
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
                child_list: None,
                function_list: None,
            })
            .collect::<Vec<_>>();

        result.push(RoleFunctionDto {
            id: module.id.to_string(),
            name: module_name,
            is_check: checked_ids.contains(&module.id),
            sign: module.sign,
            child_list: Some(children),
            function_list: Some(functions),
        });
    }

    Ok(result)
}

/// 转换数据库异常
fn to_app_error(error: DbErr) -> AppError {
    AppError::internal(error.to_string())
}
