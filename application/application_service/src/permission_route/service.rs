use application_interface::context::AppContext;
use chrono::Utc;
use common::app_error::AppError;
use repository::{extensions::SoftDeleteEntityExt, function_route};
use repository_core::versioned::VersionedActiveModel;
use sea_orm::{ActiveValue::Set, ColumnTrait, IntoActiveModel, QueryFilter, TransactionTrait};

pub use application_model::authorize::models::PermissionRouteSyncItem;

/// 权限路由应用服务
pub struct PermissionRouteService<'a> {
    /// 应用层上下文
    context: AppContext<'a>,
}

impl<'a> PermissionRouteService<'a> {
    /// 创建权限路由应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 同步当前模块的有效路由；与源 SaveChanges 一样将整批写入作为一个提交单元。
    pub async fn sync_routes(
        &self,
        module_name: &str,
        routes: &[PermissionRouteSyncItem<'_>],
    ) -> Result<bool, AppError> {
        if module_name.trim().is_empty() {
            return Err(AppError::bad_request("权限模块名不可以为空"));
        }

        // 源 EF 查询遵守软删除过滤；重新出现的路径创建新记录，不恢复旧功能绑定。
        let existing_routes = function_route::Entity::find_active()
            .filter(function_route::Column::Module.eq(module_name))
            .all(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("权限路由读取失败: {error}")))?;
        let transaction = self
            .context
            .db
            .begin()
            .await
            .map_err(|error| AppError::internal(format!("开启权限路由事务失败: {error}")))?;
        let mut changed = false;

        // 空清单也要软删除本模块的全部有效路由。
        for model in &existing_routes {
            if routes.iter().any(|route| route.route == model.route) {
                continue;
            }
            let mut active = model.clone().into_active_model();
            active.delete_time = Set(Some(Utc::now().into()));
            active
                .update_versioned(&transaction)
                .await
                .map_err(|error| AppError::internal(format!("权限路由删除标记失败: {error}")))?;
            changed = true;
        }

        for route in routes {
            if let Some(model) = existing_routes
                .iter()
                .find(|model| model.route == route.route)
            {
                if model.remark.as_deref() == Some(route.remark) {
                    continue;
                }
                let mut active = model.clone().into_active_model();
                active.remark = Set(Some(route.remark.to_owned()));
                active
                    .update_versioned(&transaction)
                    .await
                    .map_err(|error| AppError::internal(format!("权限路由更新失败: {error}")))?;
            } else {
                let route_id = self.context.id_generator.get_id().await.map_err(|error| {
                    AppError::internal(format!("生成权限路由主键失败: {error}"))
                })?;
                function_route::ActiveModel {
                    row_version: Default::default(),
                    id: Set(route_id),
                    function_id: Set(None),
                    module: Set(module_name.to_owned()),
                    route: Set(route.route.to_owned()),
                    remark: Set(Some(route.remark.to_owned())),
                    create_time: Set(Utc::now().into()),
                    delete_time: Set(None),
                }
                .insert_versioned(&transaction)
                .await
                .map_err(|error| AppError::internal(format!("权限路由创建失败: {error}")))?;
            }
            changed = true;
        }

        // 源维护方法使用普通 SaveChanges，不额外生成 DataUpdateLog。
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal(format!("提交权限路由事务失败: {error}")))?;
        Ok(changed)
    }
}
