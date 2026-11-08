use super::service::AuthorizeService;
use application_model::authorize::seed::{AuthorizeSeedData, AuthorizeSeedResult};
use chrono::Utc;
use common::{app_error::AppError, crypto_helper::hash_password_pbkdf2_sha256};
use repository::{function, function_authorize, function_route, role, user, user_role};
use repository_core::versioned::VersionedActiveModel;
use sea_orm::{
    ColumnTrait, DatabaseTransaction, DbErr, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, Set, TransactionTrait,
};
use std::collections::HashMap;

impl AuthorizeService<'_> {
    /// 初始化是显式用例；所有恢复、绑定和授权操作必须一起提交。
    pub async fn init_data(
        &self,
        seed: AuthorizeSeedData,
    ) -> Result<AuthorizeSeedResult, AppError> {
        let tx = self.context.app.db.begin().await.map_err(seed_error)?;
        let result = self.apply_seed(&tx, seed).await.map_err(seed_error)?;
        tx.commit().await.map_err(seed_error)?;
        Ok(result)
    }

    async fn seed_id(&self) -> Result<i64, DbErr> {
        self.context
            .app
            .id_generator
            .get_id()
            .await
            .map_err(|error| DbErr::Custom(format!("生成初始化数据主键失败: {error}")))
    }

    async fn apply_seed(
        &self,
        tx: &DatabaseTransaction,
        seed: AuthorizeSeedData,
    ) -> Result<AuthorizeSeedResult, DbErr> {
        // 查找包含软删除记录，重复执行恢复原记录，保留主键和创建审计。
        let existing = user::Entity::find()
            .filter(user::Column::UserName.eq(&seed.admin_user.user_name))
            .one(tx)
            .await?;
        let is_new = existing.is_none();
        let mut admin = match existing {
            Some(model) => model.into_active_model(),
            None => user::ActiveModel {
                id: Set(self.seed_id().await?),
                create_time: Set(Utc::now().into()),
                ..Default::default()
            },
        };
        let user_id = *admin.id.as_ref();
        admin.name = Set(seed.admin_user.name);
        admin.user_name = Set(seed.admin_user.user_name);
        admin.phone = Set(seed.admin_user.phone);
        admin.email = Set(seed.admin_user.email);
        admin.password = Set(hash_password_pbkdf2_sha256(
            &seed.admin_user.password,
            &user_id.to_string(),
        ));
        admin.delete_time = Set(None);
        admin.delete_user_id = Set(None);
        let admin = if is_new {
            admin.insert_versioned(tx).await?
        } else {
            admin.update_versioned(tx).await?
        };

        let existing = role::Entity::find()
            .filter(role::Column::Code.eq(&seed.admin_role.code))
            .one(tx)
            .await?;
        let is_new = existing.is_none();
        let mut admin_role = match existing {
            Some(model) => model.into_active_model(),
            None => role::ActiveModel {
                id: Set(self.seed_id().await?),
                create_time: Set(Utc::now().into()),
                ..Default::default()
            },
        };
        admin_role.code = Set(seed.admin_role.code);
        admin_role.name = Set(seed.admin_role.name);
        admin_role.remark = Set(seed.admin_role.remark);
        admin_role.delete_time = Set(None);
        let admin_role = if is_new {
            admin_role.insert_versioned(tx).await?
        } else {
            admin_role.update_versioned(tx).await?
        };
        let role_id = admin_role.id;

        let signs: Vec<_> = seed
            .functions
            .iter()
            .map(|item| item.sign.clone())
            .collect();
        let mut functions = HashMap::new();
        for model in function::Entity::find()
            .filter(function::Column::Sign.is_in(signs))
            .all(tx)
            .await?
        {
            if functions.insert(model.sign.clone(), model).is_some() {
                return Err(DbErr::Custom("初始化功能标识在数据库中重复".into()));
            }
        }
        // 先补齐全部节点，再设置父级，文件中的排列不必是拓扑顺序。
        for item in &seed.functions {
            let existing = functions.remove(&item.sign);
            let is_new = existing.is_none();
            let mut active = match existing {
                Some(model) => model.into_active_model(),
                None => function::ActiveModel {
                    id: Set(self.seed_id().await?),
                    create_time: Set(Utc::now().into()),
                    parent_id: Set(None),
                    ..Default::default()
                },
            };
            active.name = Set(item.name.clone());
            active.sign = Set(item.sign.clone());
            active.remark = Set(item.remark.clone());
            active.r#type = Set(item.r#type);
            active.delete_time = Set(None);
            let model = if is_new {
                active.insert_versioned(tx).await?
            } else {
                active.update_versioned(tx).await?
            };
            functions.insert(item.sign.clone(), model);
        }
        for item in &seed.functions {
            let parent_id = resolve_function(&functions, item.parent_sign.as_deref())?;
            let model = functions.get(&item.sign).expect("功能已在上一阶段写入");
            if model.parent_id != parent_id {
                let mut active = model.clone().into_active_model();
                active.parent_id = Set(parent_id);
                functions.insert(item.sign.clone(), active.update_versioned(tx).await?);
            }
        }

        let modules: Vec<_> = seed
            .function_routes
            .iter()
            .map(|item| item.module.clone())
            .collect();
        let routes: Vec<_> = seed
            .function_routes
            .iter()
            .map(|item| item.route.clone())
            .collect();
        let mut route_map = HashMap::new();
        for model in function_route::Entity::find()
            .filter(function_route::Column::Module.is_in(modules))
            .filter(function_route::Column::Route.is_in(routes))
            .all(tx)
            .await?
        {
            if route_map
                .insert((model.module.clone(), model.route.clone()), model)
                .is_some()
            {
                return Err(DbErr::Custom("初始化路由在数据库中重复".into()));
            }
        }
        for item in &seed.function_routes {
            let key = (item.module.clone(), item.route.clone());
            let existing = route_map.remove(&key);
            let is_new = existing.is_none();
            let mut active = match existing {
                Some(model) => model.into_active_model(),
                None => function_route::ActiveModel {
                    id: Set(self.seed_id().await?),
                    create_time: Set(Utc::now().into()),
                    ..Default::default()
                },
            };
            active.function_id = Set(resolve_function(&functions, item.function_sign.as_deref())?);
            active.module = Set(item.module.clone());
            active.route = Set(item.route.clone());
            active.remark = Set(item.remark.clone());
            active.delete_time = Set(None);
            let model = if is_new {
                active.insert_versioned(tx).await?
            } else {
                active.update_versioned(tx).await?
            };
            route_map.insert(key, model);
        }

        let existing = user_role::Entity::find()
            .filter(user_role::Column::UserId.eq(user_id))
            .filter(user_role::Column::RoleId.eq(role_id))
            .one(tx)
            .await?;
        if let Some(model) = existing {
            let mut active = model.into_active_model();
            active.delete_time = Set(None);
            active.delete_user_id = Set(None);
            active.update_versioned(tx).await?;
        } else {
            user_role::ActiveModel {
                id: Set(self.seed_id().await?),
                user_id: Set(user_id),
                role_id: Set(role_id),
                create_user_id: Set(Some(user_id)),
                create_time: Set(Utc::now().into()),
                ..Default::default()
            }
            .insert_versioned(tx)
            .await?;
        }
        for model in user_role::Entity::find()
            .filter(user_role::Column::UserId.eq(user_id))
            .filter(user_role::Column::RoleId.ne(role_id))
            .filter(user_role::Column::DeleteTime.is_null())
            .all(tx)
            .await?
        {
            let mut active = model.into_active_model();
            active.delete_time = Set(Some(Utc::now().into()));
            active.delete_user_id = Set(Some(user_id));
            active.update_versioned(tx).await?;
        }

        let mut grants = HashMap::new();
        for model in function_authorize::Entity::find()
            .filter(function_authorize::Column::RoleId.eq(role_id))
            .filter(function_authorize::Column::UserId.is_null())
            .all(tx)
            .await?
        {
            if grants.insert(model.function_id, model).is_some() {
                return Err(DbErr::Custom("管理员功能授权在数据库中重复".into()));
            }
        }
        // 包括文件之外已有的有效功能；不删除失效功能留下的历史授权。
        for function in function::Entity::find()
            .filter(function::Column::DeleteTime.is_null())
            .all(tx)
            .await?
        {
            let existing = grants.remove(&function.id);
            let is_new = existing.is_none();
            let mut active = match existing {
                Some(model) => model.into_active_model(),
                None => function_authorize::ActiveModel {
                    id: Set(self.seed_id().await?),
                    create_time: Set(Utc::now().into()),
                    create_user_id: Set(Some(user_id)),
                    ..Default::default()
                },
            };
            active.function_id = Set(function.id);
            active.role_id = Set(Some(role_id));
            active.user_id = Set(None);
            active.delete_time = Set(None);
            active.delete_user_id = Set(None);
            if is_new {
                active.insert_versioned(tx).await?;
            } else {
                active.update_versioned(tx).await?;
            }
        }

        Ok(AuthorizeSeedResult {
            admin_user: admin.user_name,
            admin_role: admin_role.code,
            function_count: seed.functions.len(),
            function_route_count: seed.function_routes.len(),
            function_authorize_count: function_authorize::Entity::find()
                .filter(function_authorize::Column::RoleId.eq(role_id))
                .filter(function_authorize::Column::UserId.is_null())
                .filter(function_authorize::Column::DeleteTime.is_null())
                .count(tx)
                .await?,
            user_role_count: user_role::Entity::find()
                .filter(user_role::Column::UserId.eq(user_id))
                .filter(user_role::Column::DeleteTime.is_null())
                .count(tx)
                .await?,
        })
    }
}

fn resolve_function(
    functions: &HashMap<String, function::Model>,
    sign: Option<&str>,
) -> Result<Option<i64>, DbErr> {
    sign.map(|sign| {
        functions
            .get(sign)
            .map(|model| model.id)
            .ok_or_else(|| DbErr::Custom(format!("初始化文件引用了未定义的功能: {sign}")))
    })
    .transpose()
}

fn seed_error(error: DbErr) -> AppError {
    AppError::internal(format!("初始化权限基础数据失败: {error}"))
}
