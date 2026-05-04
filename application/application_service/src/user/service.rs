use async_recursion::async_recursion;
use chrono::Utc;
use common::crypto_helper::hash_password_pbkdf2_sha256;
use common::string_helper::trim_prefix_name;
use repository::extensions::{SoftDeleteQueryExt, not_deleted_expr};
use repository::{
    function, function_authorize, region_area, region_city, region_province, role, user, user_info,
    user_role, user_token,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    sea_query::Condition,
};
use serde_json::json;
use source_generator_runtime::logging;
use std::collections::HashSet;

use application_interface::{
    context::{AppContext, CurrentUser},
    error::AppError,
};

use crate::write_support::{
    apply_soft_delete_audit_fields, apply_update_user_audit_field, begin_transaction,
    build_create_full_audit_fields, normalize_optional_text, normalize_page, validate_page_request,
    write_data_update_log_with_connection,
};

use super::models::{
    EditUserDto, EditUserProfileDto, PageListDto, SetUserFunctionDto, SetUserRoleDto, UserDto,
    UserFunctionDto, UserPageRequest, UserProfileDto, UserRoleDto,
};

/// 模块类型
const FUNCTION_TYPE_MODULE: i32 = 1;

/// 功能类型
const FUNCTION_TYPE_FUNCTION: i32 = 2;

/// 用户模块应用服务
pub struct UserService<'a> {
    /// 用户模块上下文
    context: AppContext<'a>,
}

impl<'a> UserService<'a> {
    /// 创建用户模块应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 校验分页请求
    fn validate_user_page_request(params: &UserPageRequest) -> Result<(), AppError> {
        validate_page_request(params.page, params.page_size)
    }

    /// 校验编辑用户请求
    fn validate_edit_user(dto: &EditUserDto) -> Result<(), AppError> {
        if dto.name.trim().is_empty() {
            return Err(AppError::bad_request("姓名不可以空"));
        }

        if dto.user_name.trim().is_empty() {
            return Err(AppError::bad_request("用户名不可以空"));
        }

        if dto.phone.trim().is_empty() {
            return Err(AppError::bad_request("手机号不可以空"));
        }

        if dto.password.is_empty() {
            return Err(AppError::bad_request("密码不可以空"));
        }

        Ok(())
    }

    /// 校验用户标识
    fn validate_user_id(target_id: i64) -> Result<(), AppError> {
        if target_id <= 0 {
            return Err(AppError::bad_request("无效的 userId"));
        }

        Ok(())
    }

    /// 解析角色标识列表
    fn parse_role_ids(role_ids: &[String]) -> Result<Vec<i64>, AppError> {
        role_ids
            .iter()
            .map(|item| {
                let role_id = item
                    .parse::<i64>()
                    .map_err(|_| AppError::bad_request("无效的 roleId"))?;

                if role_id <= 0 {
                    return Err(AppError::bad_request("无效的 roleId"));
                }

                Ok(role_id)
            })
            .collect()
    }

    /// 获取用户列表
    pub async fn get_user_list(
        &self,
        params: UserPageRequest,
    ) -> Result<PageListDto<UserDto>, AppError> {
        Self::validate_user_page_request(&params)?;

        // 关键字为空时仅按未删除条件查询
        let keyword = params
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned);
        let base_condition = build_keyword_condition(keyword.as_deref());
        let total = user::Entity::find()
            .filter(base_condition.clone())
            .count(self.context.db)
            .await
            .map_err(to_app_error)?;

        if total == 0 {
            return Ok(PageListDto {
                total: 0,
                list: vec![],
            });
        }

        let page = normalize_page(params.page);
        let skip = (page - 1) * params.page_size;
        let users = user::Entity::find()
            .filter(base_condition)
            .order_by_desc(user::Column::Id)
            .offset(skip)
            .limit(params.page_size)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        // 角色信息单独批量查询 避免列表场景下出现 N + 1 查询
        let user_ids = users.iter().map(|item| item.id).collect::<Vec<_>>();
        let (user_roles, roles) = load_roles_for_users(self.context.db, &user_ids)
            .await
            .map_err(to_app_error)?;
        let list = users
            .into_iter()
            .map(|item| build_user_dto(item, &user_roles, &roles))
            .collect::<Vec<_>>();

        Ok(PageListDto { total, list })
    }

    /// 获取单个用户
    #[logging]
    pub async fn get_user(&self, target_id: i64) -> Result<Option<UserDto>, AppError> {
        Self::validate_user_id(target_id)?;

        let user_model = user::Entity::find()
            .filter(user::Column::Id.eq(target_id))
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(user_model) = user_model else {
            return Ok(None);
        };

        let (user_roles, roles) = load_roles_for_users(self.context.db, &[user_model.id])
            .await
            .map_err(to_app_error)?;
        Ok(Some(build_user_dto(user_model, &user_roles, &roles)))
    }

    /// 获取用户角色列表
    pub async fn get_user_role_list(&self, user_id: i64) -> Result<Vec<UserRoleDto>, AppError> {
        Self::validate_user_id(user_id)?;

        if !user_exists(self.context.db, user_id)
            .await
            .map_err(to_app_error)?
        {
            return Err(AppError::bad_request("无效的 userId"));
        }

        // 角色列表全量返回 由 is_check 标记当前用户是否已分配
        let checked_role_ids = user_role::Entity::find()
            .filter(user_role::Column::UserId.eq(user_id))
            .not_deleted(user_role::Column::DeleteTime)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(|item| item.role_id)
            .collect::<HashSet<_>>();

        let role_list = role::Entity::find()
            .not_deleted(role::Column::DeleteTime)
            .order_by_desc(role::Column::Id)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        Ok(role_list
            .into_iter()
            .map(|item| UserRoleDto {
                id: item.id.to_string(),
                name: item.name,
                remark: item.remark,
                is_check: checked_role_ids.contains(&item.id),
            })
            .collect())
    }

    /// 设置用户角色
    pub async fn set_user_role(
        &self,
        dto: SetUserRoleDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        Self::validate_user_id(dto.user_id)?;

        if dto.role_id <= 0 {
            return Err(AppError::bad_request("无效的 roleId"));
        }

        let tx = begin_transaction(self.context.db).await?;

        if !user_exists(&tx, dto.user_id).await.map_err(to_app_error)? {
            return Err(AppError::bad_request("无效的 userId"));
        }

        let role_exists = role::Entity::find_by_id(dto.role_id)
            .not_deleted(role::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?
            .is_some();

        if !role_exists {
            return Err(AppError::bad_request("无效的 roleId"));
        }

        let existing = user_role::Entity::find()
            .filter(user_role::Column::UserId.eq(dto.user_id))
            .filter(user_role::Column::RoleId.eq(dto.role_id))
            .not_deleted(user_role::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?;

        // 勾选时补关系 取消时做软删除 保持和现有角色授权关系一致
        if dto.is_check {
            if existing.is_none() {
                let relation_id = self.context.id_generator.get_id().await.map_err(|error| {
                    AppError::internal(format!("生成用户角色关联主键失败: {error}"))
                })?;
                let audit_fields = build_create_full_audit_fields(current_user);

                user_role::ActiveModel {
                    id: Set(relation_id),
                    user_id: Set(dto.user_id),
                    role_id: Set(dto.role_id),
                    create_time: Set(audit_fields.create_time),
                    delete_time: Set(audit_fields.delete_time),
                    update_time: Set(audit_fields.update_time),
                    create_user_id: Set(audit_fields.create_user_id),
                    update_user_id: Set(audit_fields.update_user_id),
                    delete_user_id: Set(audit_fields.delete_user_id),
                }
                .insert(&tx)
                .await
                .map_err(|error| AppError::internal(format!("设置用户角色失败: {error}")))?;

                write_data_update_log_with_connection(
                    &tx,
                    self.context.id_generator,
                    "UserRole",
                    relation_id,
                    "create",
                    None,
                    Some(json!({
                        "id": relation_id,
                        "user_id": dto.user_id,
                        "role_id": dto.role_id,
                    })),
                    Some(current_user),
                )
                .await?;
            }
        } else if let Some(model) = existing {
            let relation_id = model.id;
            let before_log = json!({
                "id": model.id,
                "user_id": model.user_id,
                "role_id": model.role_id,
            });
            let mut active = model.into_active_model();
            apply_soft_delete_audit_fields(
                &mut active.delete_time,
                &mut active.delete_user_id,
                current_user,
            );
            active
                .update(&tx)
                .await
                .map_err(|error| AppError::internal(format!("取消用户角色失败: {error}")))?;

            write_data_update_log_with_connection(
                &tx,
                self.context.id_generator,
                "UserRole",
                relation_id,
                "delete",
                Some(before_log),
                Some(json!({
                    "user_id": dto.user_id,
                    "role_id": dto.role_id,
                })),
                Some(current_user),
            )
            .await?;
        }

        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 获取用户功能树
    pub async fn get_user_function(&self, user_id: i64) -> Result<Vec<UserFunctionDto>, AppError> {
        Self::validate_user_id(user_id)?;

        if !user_exists(self.context.db, user_id)
            .await
            .map_err(to_app_error)?
        {
            return Err(AppError::bad_request("无效的 userId"));
        }

        load_user_function_tree(self.context.db, user_id, None, None)
            .await
            .map_err(to_app_error)
    }

    /// 设置用户功能
    pub async fn set_user_function(
        &self,
        dto: SetUserFunctionDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        Self::validate_user_id(dto.user_id)?;

        if dto.function_id <= 0 {
            return Err(AppError::bad_request("无效的 functionId"));
        }

        let tx = begin_transaction(self.context.db).await?;

        if !user_exists(&tx, dto.user_id).await.map_err(to_app_error)? {
            return Err(AppError::bad_request("无效的 userId"));
        }

        let function_exists = function::Entity::find_by_id(dto.function_id)
            .not_deleted(function::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?
            .is_some();

        if !function_exists {
            return Err(AppError::bad_request("无效的 functionId"));
        }

        let role_ids = load_user_role_ids(&tx, dto.user_id)
            .await
            .map_err(to_app_error)?;

        let direct_authorize = function_authorize::Entity::find()
            .filter(function_authorize::Column::UserId.eq(dto.user_id))
            .filter(function_authorize::Column::FunctionId.eq(dto.function_id))
            .not_deleted(function_authorize::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?;

        let inherited_authorize = if role_ids.is_empty() {
            None
        } else {
            function_authorize::Entity::find()
                .filter(function_authorize::Column::RoleId.is_in(role_ids.clone()))
                .filter(function_authorize::Column::FunctionId.eq(dto.function_id))
                .not_deleted(function_authorize::Column::DeleteTime)
                .one(&tx)
                .await
                .map_err(to_app_error)?
        };

        // 用户功能允许直授 但不允许单独移除从角色继承而来的权限
        if dto.is_check {
            if direct_authorize.is_none() && inherited_authorize.is_none() {
                let authorize_id = self.context.id_generator.get_id().await.map_err(|error| {
                    AppError::internal(format!("生成功能授权主键失败: {error}"))
                })?;
                let audit_fields = build_create_full_audit_fields(current_user);

                function_authorize::ActiveModel {
                    id: Set(authorize_id),
                    function_id: Set(dto.function_id),
                    role_id: Set(None),
                    user_id: Set(Some(dto.user_id)),
                    create_time: Set(audit_fields.create_time),
                    delete_time: Set(audit_fields.delete_time),
                    update_time: Set(audit_fields.update_time),
                    create_user_id: Set(audit_fields.create_user_id),
                    update_user_id: Set(audit_fields.update_user_id),
                    delete_user_id: Set(audit_fields.delete_user_id),
                }
                .insert(&tx)
                .await
                .map_err(|error| AppError::internal(format!("设置用户功能失败: {error}")))?;

                write_data_update_log_with_connection(
                    &tx,
                    self.context.id_generator,
                    "FunctionAuthorize",
                    authorize_id,
                    "create",
                    None,
                    Some(json!({
                        "id": authorize_id,
                        "user_id": dto.user_id,
                        "function_id": dto.function_id,
                    })),
                    Some(current_user),
                )
                .await?;
            }
        } else if direct_authorize.is_some() {
            let user_authorize_list = function_authorize::Entity::find()
                .filter(function_authorize::Column::UserId.eq(dto.user_id))
                .filter(function_authorize::Column::FunctionId.eq(dto.function_id))
                .not_deleted(function_authorize::Column::DeleteTime)
                .all(&tx)
                .await
                .map_err(to_app_error)?;

            for model in user_authorize_list {
                let authorize_id = model.id;
                let function_id = model.function_id;
                let before_log = json!({
                    "id": model.id,
                    "function_id": model.function_id,
                    "role_id": model.role_id,
                    "user_id": model.user_id,
                });
                let mut active = model.into_active_model();
                apply_soft_delete_audit_fields(
                    &mut active.delete_time,
                    &mut active.delete_user_id,
                    current_user,
                );
                active
                    .update(&tx)
                    .await
                    .map_err(|error| AppError::internal(format!("取消用户功能失败: {error}")))?;

                write_data_update_log_with_connection(
                    &tx,
                    self.context.id_generator,
                    "FunctionAuthorize",
                    authorize_id,
                    "delete",
                    Some(before_log),
                    Some(json!({
                        "user_id": dto.user_id,
                        "function_id": function_id,
                    })),
                    Some(current_user),
                )
                .await?;
            }
        } else if inherited_authorize.is_some() {
            return Err(AppError::bad_request("该权限继承自角色，无法单独删除！"));
        }

        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 创建用户
    pub async fn create_user(
        &self,
        dto: EditUserDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        Self::validate_edit_user(&dto)?;
        let role_ids = Self::parse_role_ids(&dto.role_ids)?;

        // 用户基础信息 角色关系 审计日志统一放在一个事务里提交
        let tx = begin_transaction(self.context.db).await?;

        if user::Entity::find()
            .filter(user::Column::UserName.eq(&dto.user_name))
            .not_deleted(user::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?
            .is_some()
        {
            return Err(AppError::conflict("用户名已被占用，无法保存"));
        }

        let user_id = self
            .context
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成用户主键失败: {error}")))?;
        let password_hash = hash_password_pbkdf2_sha256(&dto.password, &user_id.to_string());
        let audit_after = json!({
            "id": user_id,
            "user_name": dto.user_name,
            "name": dto.name,
            "phone": dto.phone,
            "email": dto.email,
            "role_ids": dto.role_ids,
        });
        let audit_fields = build_create_full_audit_fields(current_user);

        let user_model = user::ActiveModel {
            id: Set(user_id),
            user_name: Set(dto.user_name.clone()),
            name: Set(dto.name),
            phone: Set(dto.phone),
            email: Set(dto.email),
            password: Set(password_hash),
            create_time: Set(audit_fields.create_time),
            delete_time: Set(audit_fields.delete_time),
            update_time: Set(audit_fields.update_time),
            create_user_id: Set(audit_fields.create_user_id),
            update_user_id: Set(audit_fields.update_user_id),
            delete_user_id: Set(audit_fields.delete_user_id),
        };

        user_model
            .insert(&tx)
            .await
            .map_err(|error| AppError::internal(format!("创建用户失败: {error}")))?;

        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "User",
            user_id,
            "create",
            None,
            Some(audit_after),
            Some(current_user),
        )
        .await?;

        for role_id in &role_ids {
            let Ok(relation_id) = self.context.id_generator.get_id().await else {
                continue;
            };
            let audit_fields = build_create_full_audit_fields(current_user);

            let relation = user_role::ActiveModel {
                id: Set(relation_id),
                user_id: Set(user_id),
                role_id: Set(*role_id),
                create_time: Set(audit_fields.create_time),
                delete_time: Set(audit_fields.delete_time),
                update_time: Set(audit_fields.update_time),
                create_user_id: Set(audit_fields.create_user_id),
                update_user_id: Set(audit_fields.update_user_id),
                delete_user_id: Set(audit_fields.delete_user_id),
            };

            relation
                .insert(&tx)
                .await
                .map_err(|error| AppError::internal(format!("写入用户角色失败: {error}")))?;
        }

        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(user_id.to_string())
    }

    /// 更新用户
    pub async fn update_user(
        &self,
        target_id: i64,
        dto: EditUserDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        Self::validate_user_id(target_id)?;
        Self::validate_edit_user(&dto)?;
        let new_role_ids = Self::parse_role_ids(&dto.role_ids)?;

        let tx = begin_transaction(self.context.db).await?;

        if user::Entity::find()
            .filter(user::Column::UserName.eq(&dto.user_name))
            .not_deleted(user::Column::DeleteTime)
            .filter(user::Column::Id.ne(target_id))
            .one(&tx)
            .await
            .map_err(to_app_error)?
            .is_some()
        {
            return Err(AppError::conflict("用户名已被占用，无法保存"));
        }

        let user_model = user::Entity::find()
            .filter(user::Column::Id.eq(target_id))
            .not_deleted(user::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?;

        let Some(user_model) = user_model else {
            return Err(AppError::bad_request("无效的 userId"));
        };

        let before_log = json!({
            "id": user_model.id,
            "user_name": user_model.user_name,
            "name": user_model.name,
            "phone": user_model.phone,
            "email": user_model.email,
        });
        let mut active = user_model.into_active_model();
        active.name = Set(dto.name);
        active.user_name = Set(dto.user_name.clone());
        active.phone = Set(dto.phone);
        active.email = Set(dto.email);
        apply_update_user_audit_field(&mut active.update_user_id, current_user);

        if dto.password != "default" {
            active.password = Set(hash_password_pbkdf2_sha256(
                &dto.password,
                &target_id.to_string(),
            ));
        }

        let updated_model = active
            .update(&tx)
            .await
            .map_err(|error| AppError::internal(format!("更新用户失败: {error}")))?;

        // 先取旧角色关系 再按差异执行保留 删除 新增
        let existing_roles = user_role::Entity::find()
            .filter(user_role::Column::UserId.eq(target_id))
            .not_deleted(user_role::Column::DeleteTime)
            .all(&tx)
            .await
            .map_err(to_app_error)?;

        let mut roles_to_keep = Vec::new();

        for existing in &existing_roles {
            if new_role_ids.contains(&existing.role_id) {
                roles_to_keep.push(existing.role_id);
            } else {
                let mut active = existing.clone().into_active_model();
                apply_soft_delete_audit_fields(
                    &mut active.delete_time,
                    &mut active.delete_user_id,
                    current_user,
                );
                active
                    .update(&tx)
                    .await
                    .map_err(|error| AppError::internal(format!("更新用户角色失败: {error}")))?;
            }
        }

        for role_id in &new_role_ids {
            if !roles_to_keep.contains(role_id) {
                let Ok(relation_id) = self.context.id_generator.get_id().await else {
                    continue;
                };
                let audit_fields = build_create_full_audit_fields(current_user);

                let relation = user_role::ActiveModel {
                    id: Set(relation_id),
                    user_id: Set(target_id),
                    role_id: Set(*role_id),
                    create_time: Set(audit_fields.create_time),
                    delete_time: Set(audit_fields.delete_time),
                    update_time: Set(audit_fields.update_time),
                    create_user_id: Set(audit_fields.create_user_id),
                    update_user_id: Set(audit_fields.update_user_id),
                    delete_user_id: Set(audit_fields.delete_user_id),
                };

                relation
                    .insert(&tx)
                    .await
                    .map_err(|error| AppError::internal(format!("写入用户角色失败: {error}")))?;
            }
        }

        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "User",
            target_id,
            "update",
            Some(before_log),
            Some(json!({
                "id": updated_model.id,
                "user_name": updated_model.user_name,
                "name": updated_model.name,
                "phone": updated_model.phone,
                "email": updated_model.email,
                "role_ids": new_role_ids,
            })),
            Some(current_user),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 删除用户
    pub async fn delete_user(
        &self,
        target_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        Self::validate_user_id(target_id)?;

        let user_model = user::Entity::find()
            .filter(user::Column::Id.eq(target_id))
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(user_model) = user_model else {
            return Ok(true);
        };

        let delete_time = Utc::now();
        let tx = begin_transaction(self.context.db).await?;
        let before_log = json!({
            "id": user_model.id,
            "user_name": user_model.user_name,
            "name": user_model.name,
            "phone": user_model.phone,
            "email": user_model.email,
        });
        let mut active = user_model.into_active_model();
        apply_soft_delete_audit_fields(
            &mut active.delete_time,
            &mut active.delete_user_id,
            current_user,
        );
        let _ = active
            .update(&tx)
            .await
            .map_err(|error| AppError::internal(format!("删除用户失败: {error}")))?;

        let token_list = user_token::Entity::find()
            .filter(user_token::Column::UserId.eq(target_id))
            .not_deleted(user_token::Column::DeleteTime)
            .all(&tx)
            .await
            .map_err(|error| AppError::internal(format!("查询用户 Token 失败: {error}")))?;

        for token in token_list {
            let mut token_active = token.into_active_model();
            token_active.delete_time = Set(Some(delete_time.into()));
            token_active
                .update(&tx)
                .await
                .map_err(|error| AppError::internal(format!("用户 Token 失效失败: {error}")))?;
        }

        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "User",
            target_id,
            "delete",
            Some(before_log),
            Some(json!({
                "id": target_id,
                "delete_time": delete_time.to_rfc3339(),
            })),
            Some(current_user),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 获取指定用户的资料详情
    pub async fn get_user_profile(
        &self,
        target_id: i64,
    ) -> Result<Option<UserProfileDto>, AppError> {
        Self::validate_user_id(target_id)?;

        // 资料表允许为空 但用户主记录必须先存在
        let user_exists = user::Entity::find()
            .filter(user::Column::Id.eq(target_id))
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .is_some();

        if !user_exists {
            return Ok(None);
        }

        let user_profile = user_info::Entity::find()
            .filter(user_info::Column::UserId.eq(target_id))
            .not_deleted(user_info::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        // 地区详情仅在存在区县标识时才继续级联查询
        let region_detail = match user_profile.as_ref().and_then(|item| item.region_area_id) {
            Some(region_area_id) => load_region_detail(self.context.db, region_area_id).await?,
            None => None,
        };

        Ok(Some(build_user_profile_dto(
            target_id,
            user_profile,
            region_detail,
        )))
    }

    /// 更新指定用户的资料信息
    pub async fn update_user_profile(
        &self,
        target_id: i64,
        dto: EditUserProfileDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        Self::validate_user_id(target_id)?;

        let user_exists = user::Entity::find()
            .filter(user::Column::Id.eq(target_id))
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .is_some();

        if !user_exists {
            return Err(AppError::bad_request("无效的 userId"));
        }

        if let Some(region_area_id) = dto.region_area_id {
            let region_exists = region_area::Entity::find()
                .filter(region_area::Column::Id.eq(region_area_id))
                .not_deleted(region_area::Column::DeleteTime)
                .one(self.context.db)
                .await
                .map_err(to_app_error)?
                .is_some();

            if !region_exists {
                return Err(AppError::bad_request("无效的 regionAreaId"));
            }
        }

        let existing = user_info::Entity::find()
            .filter(user_info::Column::UserId.eq(target_id))
            .not_deleted(user_info::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        if let Some(existing) = existing {
            // 已有资料时走更新路径 保留原记录主键
            let tx = begin_transaction(self.context.db).await?;
            let before_log = json!({
                "id": existing.id,
                "user_id": existing.user_id,
                "region_area_id": existing.region_area_id,
                "address": existing.address,
                "sex": existing.sex,
            });
            let mut active = existing.into_active_model();
            active.region_area_id = Set(dto.region_area_id);
            active.address = Set(normalize_optional_text(dto.address));
            active.sex = Set(dto.sex);
            apply_update_user_audit_field(&mut active.update_user_id, current_user);

            let updated_model = active
                .update(&tx)
                .await
                .map_err(|error| AppError::internal(format!("更新用户资料失败: {error}")))?;

            write_data_update_log_with_connection(
                &tx,
                self.context.id_generator,
                "UserInfo",
                updated_model.id,
                "update",
                Some(before_log),
                Some(json!({
                    "id": updated_model.id,
                    "user_id": updated_model.user_id,
                    "region_area_id": updated_model.region_area_id,
                    "address": updated_model.address,
                    "sex": updated_model.sex,
                })),
                Some(current_user),
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;
        } else {
            // 首次维护资料时直接创建一条附属记录
            let tx = begin_transaction(self.context.db).await?;
            let user_info_id =
                self.context.id_generator.get_id().await.map_err(|error| {
                    AppError::internal(format!("生成用户资料主键失败: {error}"))
                })?;
            let normalized_address = normalize_optional_text(dto.address);
            let audit_after = json!({
                "id": user_info_id,
                "user_id": target_id,
                "region_area_id": dto.region_area_id,
                "address": normalized_address,
                "sex": dto.sex,
            });
            let audit_fields = build_create_full_audit_fields(current_user);

            user_info::ActiveModel {
                id: Set(user_info_id),
                user_id: Set(target_id),
                region_area_id: Set(dto.region_area_id),
                address: Set(normalized_address),
                sex: Set(dto.sex),
                update_user_id: Set(None),
                create_time: Set(audit_fields.create_time),
                delete_time: Set(audit_fields.delete_time),
                update_time: Set(audit_fields.update_time),
            }
            .insert(&tx)
            .await
            .map_err(|error| AppError::internal(format!("创建用户资料失败: {error}")))?;

            write_data_update_log_with_connection(
                &tx,
                self.context.id_generator,
                "UserInfo",
                user_info_id,
                "create",
                None,
                Some(audit_after),
                Some(current_user),
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;
        }

        Ok(true)
    }
}

/// 构建用户查询条件
fn build_keyword_condition(keyword: Option<&str>) -> Condition {
    // 所有用户查询默认只返回未软删除数据
    let mut condition = Condition::all().add(not_deleted_expr(user::Column::DeleteTime));

    if let Some(keyword) = keyword {
        condition = condition.add(
            Condition::any()
                .add(user::Column::Name.contains(keyword))
                .add(user::Column::UserName.contains(keyword))
                .add(user::Column::Phone.contains(keyword))
                .add(user::Column::Email.contains(keyword)),
        );
    }

    condition
}

/// 检查用户是否存在
async fn user_exists<C>(db: &C, user_id: i64) -> Result<bool, DbErr>
where
    C: ConnectionTrait,
{
    user::Entity::find_by_id(user_id)
        .not_deleted(user::Column::DeleteTime)
        .one(db)
        .await
        .map(|item| item.is_some())
}

/// 加载用户角色 ID 列表
async fn load_user_role_ids<C>(db: &C, user_id: i64) -> Result<Vec<i64>, DbErr>
where
    C: ConnectionTrait,
{
    user_role::Entity::find()
        .filter(user_role::Column::UserId.eq(user_id))
        .not_deleted(user_role::Column::DeleteTime)
        .all(db)
        .await
        .map(|items| items.into_iter().map(|item| item.role_id).collect())
}

/// 批量加载用户角色信息
async fn load_roles_for_users(
    db: &DatabaseConnection,
    user_ids: &[i64],
) -> Result<(Vec<user_role::Model>, Vec<role::Model>), DbErr> {
    // 先查关系表 再按角色标识批量加载角色定义
    if user_ids.is_empty() {
        return Ok((vec![], vec![]));
    }

    let user_roles = user_role::Entity::find()
        .filter(user_role::Column::UserId.is_in(user_ids.to_vec()))
        .not_deleted(user_role::Column::DeleteTime)
        .all(db)
        .await?;

    let role_ids = user_roles
        .iter()
        .map(|item| item.role_id)
        .collect::<Vec<_>>();
    let roles = if role_ids.is_empty() {
        vec![]
    } else {
        role::Entity::find()
            .filter(role::Column::Id.is_in(role_ids))
            .not_deleted(role::Column::DeleteTime)
            .all(db)
            .await?
    };

    Ok((user_roles, roles))
}

/// 组装用户返回模型
fn build_user_dto(
    model: user::Model,
    user_roles: &[user_role::Model],
    roles: &[role::Model],
) -> UserDto {
    // 角色展示字段拼接为简短字符串 供列表直接渲染
    let user_role_ids = user_roles
        .iter()
        .filter(|item| item.user_id == model.id)
        .map(|item| item.role_id)
        .collect::<Vec<_>>();

    let role_codes = roles
        .iter()
        .filter(|item| user_role_ids.contains(&item.id))
        .map(|item| item.code.as_str())
        .collect::<Vec<_>>();

    UserDto {
        id: model.id.to_string(),
        name: model.name,
        user_name: model.user_name,
        phone: model.phone,
        email: model.email,
        roles: if role_codes.is_empty() {
            None
        } else {
            Some(role_codes.join("、"))
        },
        role_ids: user_role_ids.iter().map(|item| item.to_string()).collect(),
        create_time: model.create_time.to_rfc3339(),
    }
}

/// 递归加载用户功能树
#[async_recursion]
async fn load_user_function_tree(
    db: &DatabaseConnection,
    user_id: i64,
    parent_id: Option<i64>,
    parent_name: Option<String>,
) -> Result<Vec<UserFunctionDto>, DbErr> {
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

    let role_ids = load_user_role_ids(db, user_id).await?;
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

    let mut authorize_condition =
        Condition::any().add(function_authorize::Column::UserId.eq(user_id));

    if !role_ids.is_empty() {
        authorize_condition =
            authorize_condition.add(function_authorize::Column::RoleId.is_in(role_ids));
    }

    let checked_ids = function_authorize::Entity::find()
        .not_deleted(function_authorize::Column::DeleteTime)
        .filter(function_authorize::Column::FunctionId.is_in(all_function_ids))
        .filter(authorize_condition)
        .all(db)
        .await?
        .into_iter()
        .map(|item| item.function_id)
        .collect::<HashSet<_>>();

    let mut result = Vec::with_capacity(module_list.len());

    for module in module_list {
        let module_name = trim_prefix_name(&module.name, parent_name.as_deref());
        let children =
            load_user_function_tree(db, user_id, Some(module.id), Some(module.name.clone()))
                .await?;
        let functions = child_functions
            .iter()
            .filter(|item| item.parent_id == Some(module.id))
            .map(|item| UserFunctionDto {
                id: item.id.to_string(),
                name: trim_prefix_name(&item.name, Some(&module.name)),
                is_check: checked_ids.contains(&item.id),
                sign: item.sign.clone(),
                child_list: vec![],
                function_list: vec![],
            })
            .collect::<Vec<_>>();

        result.push(UserFunctionDto {
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

/// 用户资料关联的地区详情
struct RegionDetail {
    /// 省份信息
    province: region_province::Model,

    /// 城市信息
    city: region_city::Model,

    /// 区县信息
    area: region_area::Model,
}

/// 按区县标识加载完整地区信息
async fn load_region_detail(
    db: &DatabaseConnection,
    region_area_id: i32,
) -> Result<Option<RegionDetail>, AppError> {
    // 地区是区县到城市再到省份的三级回溯结构
    let area = region_area::Entity::find()
        .filter(region_area::Column::Id.eq(region_area_id))
        .not_deleted(region_area::Column::DeleteTime)
        .one(db)
        .await
        .map_err(to_app_error)?;

    let Some(area) = area else {
        return Ok(None);
    };

    let city = region_city::Entity::find()
        .filter(region_city::Column::Id.eq(area.city_id))
        .not_deleted(region_city::Column::DeleteTime)
        .one(db)
        .await
        .map_err(to_app_error)?;

    let Some(city) = city else {
        return Ok(None);
    };

    let province = region_province::Entity::find()
        .filter(region_province::Column::Id.eq(city.province_id))
        .not_deleted(region_province::Column::DeleteTime)
        .one(db)
        .await
        .map_err(to_app_error)?;

    let Some(province) = province else {
        return Ok(None);
    };

    Ok(Some(RegionDetail {
        province,
        city,
        area,
    }))
}

/// 用户服务核心读写路径测试
/// 组装用户资料详情返回模型
fn build_user_profile_dto(
    target_id: i64,
    profile: Option<user_info::Model>,
    region_detail: Option<RegionDetail>,
) -> UserProfileDto {
    UserProfileDto {
        user_id: target_id.to_string(),
        id: profile.as_ref().map(|item| item.id.to_string()),
        region_area_id: profile.as_ref().and_then(|item| item.region_area_id),
        region_province_id: region_detail.as_ref().map(|item| item.province.id),
        region_city_id: region_detail.as_ref().map(|item| item.city.id),
        region_province: region_detail
            .as_ref()
            .map(|item| item.province.province.clone()),
        region_city: region_detail.as_ref().map(|item| item.city.city.clone()),
        region_area: region_detail.as_ref().map(|item| item.area.area.clone()),
        address: profile.as_ref().and_then(|item| item.address.clone()),
        sex: profile.as_ref().and_then(|item| item.sex),
        create_time: profile.as_ref().map(|item| item.create_time.to_rfc3339()),
        update_time: profile
            .as_ref()
            .and_then(|item| item.update_time.map(|time| time.to_rfc3339())),
    }
}

/// 转换数据库异常
fn to_app_error(error: DbErr) -> AppError {
    AppError::internal(error.to_string())
}

mod webapi_impl {
    use distributed_cache::DistributedCacheExt;
    use distributed_lock::LockGuard;
    use repository::extensions::SoftDeleteQueryExt;

    use super::UserService;
    use crate::authorize::service::try_cache_token_validity;
    use crate::region::service::RegionWebApiService;

    use crate::write_support::{
        apply_update_user_audit_field, begin_transaction, write_data_update_log_with_connection,
    };
    use application_interface::{
        context::{CurrentUser, WebApiContext},
        error::AppError,
    };
    use application_model::region::models::RegionDto;
    use application_model::user::models::{
        EditUserDto, EditUserPhoneBySmsDto, EditUserProfileDto, PageListDto, SetUserFunctionDto,
        SetUserRoleDto, UserDto, UserFunctionDto, UserPageRequest, UserProfileDto, UserRoleDto,
    };
    use repository::{user, user_token};
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};
    use serde_json::json;
    use std::time::Duration;

    /// WebAPI 宿主用户应用服务
    pub struct UserWebApiService<'a> {
        /// 用户模块上下文
        context: WebApiContext<'a>,
    }

    impl<'a> UserWebApiService<'a> {
        /// 创建 WebAPI 宿主特化用户应用服务
        pub fn new(context: WebApiContext<'a>) -> Self {
            Self { context }
        }

        /// 构建用户名互斥锁键
        fn build_user_name_lock_key(user_name: &str) -> Option<String> {
            let normalized = user_name.trim().to_lowercase();
            if normalized.is_empty() {
                return None;
            }

            Some(format!("userName:{normalized}"))
        }

        /// 尝试获取用户名互斥锁，防止并发创建/更新时重复通过查重
        async fn try_lock_user_name(&self, user_name: &str) -> Result<Option<LockGuard>, AppError> {
            let Some(lock_key) = Self::build_user_name_lock_key(user_name) else {
                return Ok(None);
            };

            self.context
                .distributed_lock
                .try_lock(&lock_key, Duration::ZERO, 1)
                .await
                .map_err(AppError::from)
        }

        /// 获取用户列表
        pub async fn get_user_list(
            &self,
            params: UserPageRequest,
        ) -> Result<PageListDto<UserDto>, AppError> {
            UserService::new(self.context.app)
                .get_user_list(params)
                .await
        }

        /// 获取单个用户
        pub async fn get_user(&self, target_id: i64) -> Result<Option<UserDto>, AppError> {
            UserService::new(self.context.app).get_user(target_id).await
        }

        /// 获取用户角色列表
        pub async fn get_user_role_list(
            &self,
            target_id: i64,
        ) -> Result<Vec<UserRoleDto>, AppError> {
            UserService::new(self.context.app)
                .get_user_role_list(target_id)
                .await
        }

        /// 设置用户角色
        pub async fn set_user_role(
            &self,
            dto: SetUserRoleDto,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let target_id = dto.user_id;
            let result = UserService::new(self.context.app)
                .set_user_role(dto, current_user)
                .await?;
            self.invalidate_user_permission_cache(target_id).await;
            Ok(result)
        }

        /// 获取用户功能树
        pub async fn get_user_function(
            &self,
            target_id: i64,
        ) -> Result<Vec<UserFunctionDto>, AppError> {
            UserService::new(self.context.app)
                .get_user_function(target_id)
                .await
        }

        /// 设置用户功能
        pub async fn set_user_function(
            &self,
            dto: SetUserFunctionDto,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let target_id = dto.user_id;
            let result = UserService::new(self.context.app)
                .set_user_function(dto, current_user)
                .await?;
            self.invalidate_user_permission_cache(target_id).await;
            Ok(result)
        }

        /// 创建用户
        pub async fn create_user(
            &self,
            dto: EditUserDto,
            current_user: &CurrentUser,
        ) -> Result<String, AppError> {
            let _user_name_lock_guard = if dto.user_name.trim().is_empty() {
                None
            } else {
                let lock_guard = self.try_lock_user_name(&dto.user_name).await?;
                if lock_guard.is_none() {
                    return Err(AppError::conflict("用户名已被占用，无法保存"));
                }

                lock_guard
            };

            let user_id = UserService::new(self.context.app)
                .create_user(dto, current_user)
                .await?;
            if let Ok(target_id) = user_id.parse::<i64>() {
                self.invalidate_user_permission_cache(target_id).await;
            }
            Ok(user_id)
        }

        /// 更新用户
        pub async fn update_user(
            &self,
            target_id: i64,
            dto: EditUserDto,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let _user_name_lock_guard = if dto.user_name.trim().is_empty() {
                None
            } else {
                let lock_guard = self.try_lock_user_name(&dto.user_name).await?;
                if lock_guard.is_none() {
                    return Err(AppError::conflict("用户名已被占用，无法保存"));
                }

                lock_guard
            };

            let result = UserService::new(self.context.app)
                .update_user(target_id, dto, current_user)
                .await?;
            self.invalidate_user_permission_cache(target_id).await;
            Ok(result)
        }

        /// 删除用户
        pub async fn delete_user(
            &self,
            target_id: i64,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let token_ids = user_token::Entity::find()
                .filter(user_token::Column::UserId.eq(target_id))
                .not_deleted(user_token::Column::DeleteTime)
                .all(self.context.app.db)
                .await
                .map_err(to_app_error)?
                .into_iter()
                .map(|token| token.id)
                .collect::<Vec<_>>();

            let result = UserService::new(self.context.app)
                .delete_user(target_id, current_user)
                .await?;

            for token_id in token_ids {
                try_cache_token_validity(self.context.cache, target_id, token_id, false).await;
            }

            self.invalidate_user_permission_cache(target_id).await;
            Ok(result)
        }

        /// 获取指定用户资料
        pub async fn get_user_profile(
            &self,
            target_id: i64,
        ) -> Result<Option<UserProfileDto>, AppError> {
            UserService::new(self.context.app)
                .get_user_profile(target_id)
                .await
        }

        /// 更新指定用户资料
        pub async fn update_user_profile(
            &self,
            target_id: i64,
            dto: EditUserProfileDto,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            UserService::new(self.context.app)
                .update_user_profile(target_id, dto, current_user)
                .await
        }

        /// 获取省份字典列表
        pub async fn get_region_province_list(&self) -> Result<Vec<RegionDto>, AppError> {
            RegionWebApiService::new(self.context).get_provinces().await
        }

        /// 获取指定省份下的城市字典列表
        pub async fn get_region_city_list(
            &self,
            province_id: i32,
        ) -> Result<Vec<RegionDto>, AppError> {
            RegionWebApiService::new(self.context)
                .get_cities(province_id)
                .await
        }

        /// 获取指定城市下的区县字典列表
        pub async fn get_region_area_list(&self, city_id: i32) -> Result<Vec<RegionDto>, AppError> {
            RegionWebApiService::new(self.context)
                .get_areas(city_id)
                .await
        }

        /// 校验短信修改手机号请求
        fn validate_edit_user_phone_by_sms_request(
            request: &EditUserPhoneBySmsDto,
        ) -> Result<(), AppError> {
            if request.new_phone.trim().is_empty() {
                return Err(AppError::bad_request("新手机号不可以空"));
            }

            if request.sms_code.trim().is_empty() {
                return Err(AppError::bad_request("短信验证码不可以空"));
            }

            Ok(())
        }

        /// 通过短信验证码修改当前用户手机号
        pub async fn edit_user_phone_by_sms(
            &self,
            request: EditUserPhoneBySmsDto,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            Self::validate_edit_user_phone_by_sms_request(&request)?;

            let Some(cache) = self.context.cache else {
                return Err(AppError::internal("当前宿主未启用缓存，无法校验短信验证码"));
            };

            let cache_key = format!("VerifyPhone_{}", request.new_phone.trim());
            let cached_code = cache.get_string(&cache_key).await?;

            match cached_code.as_deref() {
                Some(code) if code == request.sms_code.trim() => {
                    cache.remove(&cache_key).await?;
                }
                _ => return Err(AppError::bad_request("短信验证码错误")),
            }

            let user_model = user::Entity::find_by_id(current_user.user_id)
                .not_deleted(user::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(to_app_error)?;

            let Some(user_model) = user_model else {
                return Err(AppError::bad_request("账户不存在"));
            };

            let phone_exists = user::Entity::find()
                .filter(user::Column::Id.ne(current_user.user_id))
                .filter(user::Column::Phone.eq(request.new_phone.trim()))
                .not_deleted(user::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(to_app_error)?
                .is_some();

            if phone_exists {
                return Err(AppError::conflict("手机号已被其他账户绑定"));
            }

            let before_log = json!({
                "id": user_model.id,
                "phone": user_model.phone,
            });
            let tx = begin_transaction(self.context.app.db).await?;
            let mut active = user_model.into_active_model();
            active.phone = Set(request.new_phone.trim().to_string());
            apply_update_user_audit_field(&mut active.update_user_id, current_user);
            let updated_model = active.update(&tx).await.map_err(to_app_error)?;

            write_data_update_log_with_connection(
                &tx,
                self.context.app.id_generator,
                "User",
                current_user.user_id,
                "update_phone_by_sms",
                Some(before_log),
                Some(json!({
                    "id": updated_model.id,
                    "phone": updated_model.phone,
                })),
                Some(current_user),
            )
            .await?;
            tx.commit()
                .await
                .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

            self.invalidate_user_permission_cache(current_user.user_id)
                .await;

            Ok(true)
        }

        /// 使指定用户的权限缓存失效
        async fn invalidate_user_permission_cache(&self, user_id: i64) {
            crate::authorize::service::invalidate_user_authorize_cache(self.context.cache, user_id)
                .await;
        }
    }

    /// 转换数据库异常
    fn to_app_error(error: sea_orm::DbErr) -> AppError {
        AppError::internal(error.to_string())
    }
}

pub use webapi_impl::UserWebApiService;
