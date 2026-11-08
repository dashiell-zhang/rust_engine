use async_recursion::async_recursion;
use common::crypto_helper::hash_password_pbkdf2_sha256;
use common::string_helper::trim_prefix_name;
use repository::extensions::{SoftDeleteQueryExt, not_deleted_expr};
use repository::{
    function, function_authorize, region_area, region_city, region_province, role, user, user_info,
    user_role,
};
use repository_core::pagination::SignedPage;
use repository_core::versioned::VersionedActiveModel;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, IntoActiveModel, PaginatorTrait,
    QueryFilter, QueryOrder, Set, sea_query::Condition,
};
use serde_json::json;
use std::collections::HashSet;

use application_interface::error::AppError;
use application_runtime::context::{AppContext, CurrentUser};

use crate::write_support::{
    apply_soft_delete_audit_fields, apply_update_user_audit_field, begin_transaction,
    build_create_full_audit_fields, normalize_optional_text, page_offset, validate_page_request,
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
            return Err(AppError::bad_request("名称不可以空"));
        }

        if dto.user_name.trim().is_empty() {
            return Err(AppError::bad_request("用户名不可以空"));
        }

        if dto.phone.trim().is_empty() {
            return Err(AppError::bad_request("手机号不可以空"));
        }

        if dto.password.trim().is_empty() {
            return Err(AppError::bad_request("密码不可以为空"));
        }

        if dto.role_ids.is_empty() {
            return Err(AppError::bad_request("至少选择一个角色"));
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
    fn parse_role_ids(role_ids: &[Option<String>]) -> Result<Vec<i64>, AppError> {
        role_ids
            .iter()
            .map(|item| {
                item.as_deref()
                    .ok_or_else(|| AppError::internal("角色 ID 不可以为 null"))?
                    .trim()
                    .parse::<i64>()
                    .map_err(|error| AppError::internal(format!("解析角色 ID 失败: {error}")))
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
        let total = repository_core::pagination::count_i32(total).map_err(to_app_error)?;

        if total == 0 {
            return Ok(PageListDto {
                total: 0,
                list: None,
            });
        }

        let skip = page_offset(params.page, params.page_size);
        let users = user::Entity::find()
            .filter(base_condition)
            .order_by_desc(user::Column::Id)
            .signed_page(skip, params.page_size)
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
            .map(|item| build_user_dto(item, &user_roles, &roles, "、"))
            .collect::<Vec<_>>();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }

    /// 获取单个用户
    pub async fn get_user(&self, target_id: i64) -> Result<Option<UserDto>, AppError> {
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
        let mut dto = build_user_dto(user_model, &user_roles, &roles, ",");
        dto.role_ids = None;
        Ok(Some(dto))
    }

    /// 获取用户角色列表
    pub async fn get_user_role_list(&self, user_id: i64) -> Result<Vec<UserRoleDto>, AppError> {
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
        let tx = begin_transaction(self.context.db).await?;

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
                    row_version: Default::default(),
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
                .insert_versioned(&tx)
                .await
                .map_err(|error| AppError::internal(format!("设置用户角色失败: {error}")))?;
            }
        } else if let Some(model) = existing {
            let mut active = model.into_active_model();
            apply_soft_delete_audit_fields(
                &mut active.delete_time,
                &mut active.delete_user_id,
                current_user,
            );
            active
                .update_versioned(&tx)
                .await
                .map_err(|error| AppError::internal(format!("取消用户角色失败: {error}")))?;
        }

        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 获取用户功能树
    pub async fn get_user_function(&self, user_id: i64) -> Result<Vec<UserFunctionDto>, AppError> {
        let role_ids = load_user_role_ids(self.context.db, user_id)
            .await
            .map_err(to_app_error)?;
        load_user_function_tree(self.context.db, user_id, &role_ids, None, None)
            .await
            .map_err(to_app_error)
    }

    /// 设置用户功能
    pub async fn set_user_function(
        &self,
        dto: SetUserFunctionDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let tx = begin_transaction(self.context.db).await?;
        let role_ids = load_user_role_ids(&tx, dto.user_id)
            .await
            .map_err(to_app_error)?;
        let mut owner_condition =
            Condition::any().add(function_authorize::Column::UserId.eq(dto.user_id));
        if !role_ids.is_empty() {
            owner_condition =
                owner_condition.add(function_authorize::Column::RoleId.is_in(role_ids));
        }
        let existing = function_authorize::Entity::find()
            .not_deleted(function_authorize::Column::DeleteTime)
            .filter(function_authorize::Column::FunctionId.eq(dto.function_id))
            .filter(owner_condition)
            .one(&tx)
            .await
            .map_err(to_app_error)?;

        // 用户功能允许直授 但不允许单独移除从角色继承而来的权限
        if dto.is_check {
            if existing.is_none() {
                let authorize_id = self.context.id_generator.get_id().await.map_err(|error| {
                    AppError::internal(format!("生成功能授权主键失败: {error}"))
                })?;
                let audit_fields = build_create_full_audit_fields(current_user);

                function_authorize::ActiveModel {
                    row_version: Default::default(),
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
                .insert_versioned(&tx)
                .await
                .map_err(|error| AppError::internal(format!("设置用户功能失败: {error}")))?;
            }
        } else if let Some(model) = existing {
            if model.role_id.is_some() {
                return Err(AppError::bad_request("该权限继承自角色，无法单独删除！"));
            }
            let mut active = model.into_active_model();
            apply_soft_delete_audit_fields(
                &mut active.delete_time,
                &mut active.delete_user_id,
                current_user,
            );
            active
                .update_versioned(&tx)
                .await
                .map_err(|error| AppError::internal(format!("取消用户功能失败: {error}")))?;
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

        // 用户基础信息与角色关系在同一事务中提交
        let tx = begin_transaction(self.context.db).await?;

        if user::Entity::find()
            .filter(user::Column::UserName.eq(&dto.user_name))
            .not_deleted(user::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?
            .is_some()
        {
            return Err(AppError::bad_request("用户名已被占用,无法保存"));
        }

        let role_ids = Self::parse_role_ids(&dto.role_ids)?;

        let user_id = self
            .context
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成用户主键失败: {error}")))?;
        let password_hash = hash_password_pbkdf2_sha256(&dto.password, &user_id.to_string());
        let audit_fields = build_create_full_audit_fields(current_user);

        let user_model = user::ActiveModel {
            row_version: Default::default(),
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
            .insert_versioned(&tx)
            .await
            .map_err(|error| AppError::internal(format!("创建用户失败: {error}")))?;

        for role_id in &role_ids {
            let relation_id =
                self.context.id_generator.get_id().await.map_err(|error| {
                    AppError::internal(format!("生成用户角色主键失败: {error}"))
                })?;
            let audit_fields = build_create_full_audit_fields(current_user);

            let relation = user_role::ActiveModel {
                row_version: Default::default(),
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
                .insert_versioned(&tx)
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
        Self::validate_edit_user(&dto)?;

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
            return Err(AppError::bad_request("用户名已被占用,无法保存"));
        }

        let mut new_role_ids = Self::parse_role_ids(&dto.role_ids)?;

        let user_model = user::Entity::find()
            .filter(user::Column::Id.eq(target_id))
            .not_deleted(user::Column::DeleteTime)
            .one(&tx)
            .await
            .map_err(to_app_error)?;

        let Some(user_model) = user_model else {
            return Err(AppError::bad_request("无效的UserId"));
        };

        let password = if dto.password == "default" {
            user_model.password.clone()
        } else {
            hash_password_pbkdf2_sha256(&dto.password, &target_id.to_string())
        };
        // 源仓储仅保存实际改变的实体；仅调整角色时不刷新用户版本。
        let user_changed = user_model.name != dto.name
            || user_model.user_name != dto.user_name
            || user_model.phone != dto.phone
            || user_model.email != dto.email
            || user_model.password != password
            || user_model.update_user_id != Some(current_user.user_id);
        if user_changed {
            let mut active = user_model.into_active_model();
            active.name = Set(dto.name);
            active.user_name = Set(dto.user_name);
            active.phone = Set(dto.phone);
            active.email = Set(dto.email);
            active.password = Set(password);
            apply_update_user_audit_field(&mut active.update_user_id, current_user);
            active
                .update_versioned(&tx)
                .await
                .map_err(|error| AppError::internal(format!("更新用户失败: {error}")))?;
        }

        // 先取旧角色关系 再按差异执行保留 删除 新增
        let existing_roles = user_role::Entity::find()
            .filter(user_role::Column::UserId.eq(target_id))
            .not_deleted(user_role::Column::DeleteTime)
            .all(&tx)
            .await
            .map_err(to_app_error)?;

        for existing in &existing_roles {
            if let Some(index) = new_role_ids.iter().position(|id| *id == existing.role_id) {
                new_role_ids.remove(index);
            } else {
                let mut active = existing.clone().into_active_model();
                apply_soft_delete_audit_fields(
                    &mut active.delete_time,
                    &mut active.delete_user_id,
                    current_user,
                );
                active
                    .update_versioned(&tx)
                    .await
                    .map_err(|error| AppError::internal(format!("更新用户角色失败: {error}")))?;
            }
        }

        for role_id in &new_role_ids {
            let relation_id =
                self.context.id_generator.get_id().await.map_err(|error| {
                    AppError::internal(format!("生成用户角色主键失败: {error}"))
                })?;
            let audit_fields = build_create_full_audit_fields(current_user);

            let relation = user_role::ActiveModel {
                row_version: Default::default(),
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
                .insert_versioned(&tx)
                .await
                .map_err(|error| AppError::internal(format!("写入用户角色失败: {error}")))?;
        }

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
        let user_model = user::Entity::find()
            .filter(user::Column::Id.eq(target_id))
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(user_model) = user_model else {
            return Ok(true);
        };

        let tx = begin_transaction(self.context.db).await?;
        let mut active = user_model.into_active_model();
        apply_soft_delete_audit_fields(
            &mut active.delete_time,
            &mut active.delete_user_id,
            current_user,
        );
        let _ = active
            .update_versioned(&tx)
            .await
            .map_err(|error| AppError::internal(format!("删除用户失败: {error}")))?;

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
                .update_versioned(&tx)
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
                row_version: Default::default(),
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
            .insert_versioned(&tx)
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
        // Contains 按字面子串搜索，不能把用户输入当作 SQL LIKE 通配符。
        let pattern = repository_core::extensions::contains_pattern(keyword);
        condition = condition.add(
            Condition::any()
                .add(user::Column::Name.like(pattern.clone()))
                .add(user::Column::UserName.like(pattern.clone()))
                .add(user::Column::Phone.like(pattern.clone()))
                .add(user::Column::Email.like(pattern.clone())),
        );
    }

    condition
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

/// 组装用户返回模型，按用户角色关系顺序关联有效角色。
fn build_user_dto(
    model: user::Model,
    user_roles: &[user_role::Model],
    roles: &[role::Model],
    role_separator: &str,
) -> UserDto {
    let assigned_roles = user_roles
        .iter()
        .filter(|item| item.user_id == model.id)
        .filter_map(|item| roles.iter().find(|role| role.id == item.role_id))
        .collect::<Vec<_>>();
    let role_codes = assigned_roles
        .iter()
        .map(|item| item.code.as_str())
        .collect::<Vec<_>>();

    UserDto {
        id: model.id.to_string(),
        name: model.name,
        user_name: model.user_name,
        phone: model.phone,
        email: model.email,
        roles: Some(role_codes.join(role_separator)),
        role_ids: Some(
            assigned_roles
                .iter()
                .map(|item| item.id.to_string())
                .collect(),
        ),
        create_time: model.create_time,
    }
}

/// 递归加载用户功能树
#[async_recursion]
async fn load_user_function_tree(
    db: &DatabaseConnection,
    user_id: i64,
    role_ids: &[i64],
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
        authorize_condition = authorize_condition
            .add(function_authorize::Column::RoleId.is_in(role_ids.iter().copied()));
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
        let children = load_user_function_tree(
            db,
            user_id,
            role_ids,
            Some(module.id),
            Some(module.name.clone()),
        )
        .await?;
        let functions = child_functions
            .iter()
            .filter(|item| item.parent_id == Some(module.id))
            .map(|item| UserFunctionDto {
                id: item.id.to_string(),
                name: trim_prefix_name(&item.name, Some(&module.name)),
                is_check: checked_ids.contains(&item.id),
                sign: item.sign.clone(),
                child_list: None,
                function_list: None,
            })
            .collect::<Vec<_>>();

        result.push(UserFunctionDto {
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
    use crate::region::service::RegionWebApiService;

    use application_interface::error::AppError;
    use application_model::region::models::RegionDto;
    use application_model::user::models::{
        EditUserDto, EditUserPhoneBySmsDto, EditUserProfileDto, PageListDto, SetUserFunctionDto,
        SetUserRoleDto, UserDto, UserFunctionDto, UserPageRequest, UserProfileDto, UserRoleDto,
    };
    use application_runtime::context::{CurrentUser, WebApiContext};
    use repository::user;
    use repository_core::versioned::VersionedActiveModel;
    use sea_orm::{ColumnTrait, IntoActiveModel, QueryFilter, Set};
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
            let normalized = user_name.to_lowercase();
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
            UserService::new(self.context.app)
                .set_user_role(dto, current_user)
                .await
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
            UserService::new(self.context.app)
                .set_user_function(dto, current_user)
                .await
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
                    return Err(AppError::bad_request("用户名已被占用,无法保存"));
                }

                lock_guard
            };

            UserService::new(self.context.app)
                .create_user(dto, current_user)
                .await
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
                    return Err(AppError::bad_request("用户名已被占用,无法保存"));
                }

                lock_guard
            };

            UserService::new(self.context.app)
                .update_user(target_id, dto, current_user)
                .await
        }

        /// 删除用户
        pub async fn delete_user(
            &self,
            target_id: i64,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            UserService::new(self.context.app)
                .delete_user(target_id, current_user)
                .await
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

        /// 通过短信验证码修改当前用户手机号
        pub async fn edit_user_phone_by_sms(
            &self,
            request: EditUserPhoneBySmsDto,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let Some(cache) = self.context.cache else {
                return Err(AppError::internal("当前宿主未启用缓存，无法校验短信验证码"));
            };

            let cache_key = format!("VerifyPhone_{}", request.new_phone);
            let cached_code = cache.get_string(&cache_key).await?;

            match cached_code.as_deref() {
                Some(code) if !code.is_empty() && code == request.sms_code => {}
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
                .filter(user::Column::Phone.eq(&request.new_phone))
                .not_deleted(user::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(to_app_error)?
                .is_some();

            if phone_exists {
                return Err(AppError::bad_request("手机号已被其他账户绑定"));
            }

            // 与源仓储的变更跟踪一致：相同值不更新版本及审计时间。
            if user_model.phone == request.new_phone {
                return Ok(true);
            }
            let mut active = user_model.into_active_model();
            active.phone = Set(request.new_phone);
            active
                .update_versioned(self.context.app.db)
                .await
                .map_err(to_app_error)?;

            Ok(true)
        }
    }

    /// 转换数据库异常
    fn to_app_error(error: sea_orm::DbErr) -> AppError {
        AppError::internal(error.to_string())
    }
}

pub use webapi_impl::UserWebApiService;
