use application_interface::context::AppContext;
use chrono::Utc;
use common::app_error::AppError;
use repository::function_route;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use serde_json::json;

use crate::write_support::write_data_update_log;

/// 权限路由同步数据
pub struct PermissionRouteSyncItem<'a> {
    /// 路由路径
    pub route: &'a str,

    /// 路由说明
    pub remark: &'a str,
}

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

    /// 同步接口权限路由到数据库
    pub async fn sync_routes(
        &self,
        module_name: &str,
        routes: &[PermissionRouteSyncItem<'_>],
    ) -> Result<(), AppError> {
        if module_name.trim().is_empty() {
            return Err(AppError::bad_request("权限模块名不可以为空"));
        }

        if routes.is_empty() {
            return Ok(());
        }

        let existing_routes = function_route::Entity::find()
            .filter(function_route::Column::Module.eq(module_name))
            .all(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("权限路由读取失败: {error}")))?;

        let route_paths = routes.iter().map(|item| item.route).collect::<Vec<_>>();

        for model in &existing_routes {
            if model.delete_time.is_some() {
                continue;
            }

            if route_paths.contains(&model.route.as_str()) {
                continue;
            }

            let mut active = model.clone().into_active_model();
            active.delete_time = Set(Some(Utc::now().into()));
            let updated = active
                .update(self.context.db)
                .await
                .map_err(|error| AppError::internal(format!("权限路由删除标记失败: {error}")))?;

            write_data_update_log(
                self.context,
                "FunctionRoute",
                updated.id,
                "delete",
                Some(json!({
                    "id": model.id,
                    "module": model.module,
                    "route": model.route,
                    "remark": model.remark,
                    "delete_time": model.delete_time.map(|item| item.to_rfc3339()),
                })),
                Some(json!({
                    "id": updated.id,
                    "module": updated.module,
                    "route": updated.route,
                    "remark": updated.remark,
                    "delete_time": updated.delete_time.map(|item| item.to_rfc3339()),
                })),
                None,
            )
            .await?;
        }

        for route in routes {
            let existing = existing_routes
                .iter()
                .find(|item| item.route == route.route);

            if let Some(model) = existing {
                let should_update_remark = model.remark.as_deref() != Some(route.remark);
                let should_restore = model.delete_time.is_some();

                if !should_update_remark && !should_restore {
                    continue;
                }

                let mut active = model.clone().into_active_model();
                active.remark = Set(Some(route.remark.to_string()));
                active.delete_time = Set(None);
                let updated = active
                    .update(self.context.db)
                    .await
                    .map_err(|error| AppError::internal(format!("权限路由更新失败: {error}")))?;

                write_data_update_log(
                    self.context,
                    "FunctionRoute",
                    updated.id,
                    "update",
                    Some(json!({
                        "id": model.id,
                        "module": model.module,
                        "route": model.route,
                        "remark": model.remark,
                        "delete_time": model.delete_time.map(|item| item.to_rfc3339()),
                    })),
                    Some(json!({
                        "id": updated.id,
                        "module": updated.module,
                        "route": updated.route,
                        "remark": updated.remark,
                        "delete_time": updated.delete_time.map(|item| item.to_rfc3339()),
                    })),
                    None,
                )
                .await?;
                continue;
            }

            let route_id =
                self.context.id_generator.get_id().await.map_err(|error| {
                    AppError::internal(format!("生成权限路由主键失败: {error}"))
                })?;

            let created = function_route::ActiveModel {
                id: Set(route_id),
                function_id: Set(None),
                module: Set(module_name.to_string()),
                route: Set(route.route.to_string()),
                remark: Set(Some(route.remark.to_string())),
                create_time: Set(Utc::now().into()),
                delete_time: Set(None),
            }
            .insert(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("权限路由创建失败: {error}")))?;

            write_data_update_log(
                self.context,
                "FunctionRoute",
                created.id,
                "create",
                None,
                Some(json!({
                    "id": created.id,
                    "module": created.module,
                    "route": created.route,
                    "remark": created.remark,
                    "delete_time": created.delete_time.map(|item| item.to_rfc3339()),
                })),
                None,
            )
            .await?;
        }

        Ok(())
    }
}
