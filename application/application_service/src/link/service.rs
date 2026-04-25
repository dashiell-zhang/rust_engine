use repository::extensions::SoftDeleteQueryExt;
use repository::link;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde_json::json;

use application_interface::{
    context::{AppContext, CurrentUser},
    error::AppError,
};

use crate::user::models::PageListDto;
use crate::write_support::{
    apply_soft_delete_audit_fields, build_create_audit_fields, normalize_page,
    validate_page_request, write_data_update_log,
};

use super::models::{EditLinkDto, LinkDto, LinkPageListDto, LinkPageRequest, PublicLinkDto};

/// 友情链接应用服务
pub struct LinkService<'a> {
    /// 应用层上下文
    context: AppContext<'a>,
}

impl<'a> LinkService<'a> {
    /// 创建友情链接应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 获取友情链接分页列表
    pub async fn get_link_list(
        &self,
        params: LinkPageRequest,
    ) -> Result<LinkPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let query = link::Entity::find().not_deleted(link::Column::DeleteTime);

        let total = query
            .clone()
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
        let list = query
            .order_by_desc(link::Column::Id)
            .offset(skip)
            .limit(params.page_size)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(to_link_dto)
            .collect();

        Ok(PageListDto { total, list })
    }

    /// 获取单个友情链接
    pub async fn get_link(&self, link_id: i64) -> Result<Option<LinkDto>, AppError> {
        if link_id <= 0 {
            return Err(AppError::bad_request("无效的 linkId"));
        }

        let model = link::Entity::find()
            .filter(link::Column::Id.eq(link_id))
            .not_deleted(link::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        Ok(model.map(to_link_dto))
    }

    /// 获取客户端友情链接列表，按排序值升序返回
    pub async fn get_public_link_list(&self) -> Result<Vec<PublicLinkDto>, AppError> {
        link::Entity::find()
            .not_deleted(link::Column::DeleteTime)
            .order_by_asc(link::Column::Sort)
            .order_by_desc(link::Column::Id)
            .all(self.context.db)
            .await
            .map_err(to_app_error)
            .map(|items| {
                items
                    .into_iter()
                    .map(to_public_link_dto)
                    .collect::<Vec<_>>()
            })
    }

    /// 创建友情链接，返回新链接 ID
    pub async fn create_link(
        &self,
        dto: EditLinkDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        validate_edit_link(&dto)?;

        let id = self
            .context
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成友情链接主键失败: {error}")))?;
        let audit_after = json!({
            "id": id,
            "name": dto.name,
            "url": dto.url,
            "sort": dto.sort,
            "remark": dto.remark,
        });
        let audit_fields = build_create_audit_fields(current_user);
        let model = link::ActiveModel {
            id: Set(id),
            name: Set(dto.name),
            url: Set(dto.url),
            sort: Set(dto.sort),
            remark: Set(dto.remark),
            create_time: Set(audit_fields.create_time),
            delete_time: Set(audit_fields.delete_time),
            create_user_id: Set(audit_fields.create_user_id),
            delete_user_id: Set(audit_fields.delete_user_id),
        };

        model
            .insert(self.context.db)
            .await
            .map_err(|e| AppError::internal(format!("创建链接失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Link",
            id,
            "create",
            None,
            Some(audit_after),
            Some(current_user),
        )
        .await?;

        Ok(id.to_string())
    }

    /// 更新友情链接信息
    pub async fn update_link(
        &self,
        link_id: i64,
        dto: EditLinkDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        if link_id <= 0 {
            return Err(AppError::bad_request("无效的 linkId"));
        }

        validate_edit_link(&dto)?;

        let model = link::Entity::find()
            .filter(link::Column::Id.eq(link_id))
            .not_deleted(link::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Err(AppError::bad_request("无效的 linkId"));
        };

        let before_log = json!({
            "id": model.id,
            "name": model.name,
            "url": model.url,
            "sort": model.sort,
            "remark": model.remark,
        });
        let mut active = model.into_active_model();
        active.name = Set(dto.name);
        active.url = Set(dto.url);
        active.sort = Set(dto.sort);
        active.remark = Set(dto.remark);

        let updated_model = active
            .update(self.context.db)
            .await
            .map_err(|e| AppError::internal(format!("更新链接失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Link",
            link_id,
            "update",
            Some(before_log),
            Some(json!({
                "id": updated_model.id,
                "name": updated_model.name,
                "url": updated_model.url,
                "sort": updated_model.sort,
                "remark": updated_model.remark,
            })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 软删除友情链接
    pub async fn delete_link(
        &self,
        link_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        if link_id <= 0 {
            return Err(AppError::bad_request("无效的 linkId"));
        }

        let model = link::Entity::find()
            .filter(link::Column::Id.eq(link_id))
            .not_deleted(link::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(true);
        };

        let before_log = json!({
            "id": model.id,
            "name": model.name,
            "url": model.url,
            "sort": model.sort,
            "remark": model.remark,
        });
        let mut active = model.into_active_model();
        apply_soft_delete_audit_fields(
            &mut active.delete_time,
            &mut active.delete_user_id,
            current_user,
        );

        active
            .update(self.context.db)
            .await
            .map_err(|e| AppError::internal(format!("删除链接失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Link",
            link_id,
            "delete",
            Some(before_log),
            Some(json!({
                "id": link_id,
                "delete_user_id": current_user.user_id,
            })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }
}

/// 校验编辑链接请求
fn validate_edit_link(dto: &EditLinkDto) -> Result<(), AppError> {
    if dto.name.trim().is_empty() {
        return Err(AppError::bad_request("链接名称不可以为空"));
    }

    if dto.url.trim().is_empty() {
        return Err(AppError::bad_request("链接地址不可以为空"));
    }

    Ok(())
}

/// 将实体模型转换为返回模型
fn to_link_dto(model: link::Model) -> LinkDto {
    LinkDto {
        id: model.id.to_string(),
        name: model.name,
        url: model.url,
        sort: model.sort,
        remark: model.remark,
        create_time: model.create_time.to_rfc3339(),
    }
}

/// 将实体模型转换为客户端返回模型
fn to_public_link_dto(model: link::Model) -> PublicLinkDto {
    PublicLinkDto {
        id: model.id.to_string(),
        name: model.name,
        url: model.url,
        sort: model.sort,
        remark: model.remark,
    }
}

/// 将数据库错误转换为应用层错误
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}

#[allow(clippy::collapsible_if)]
mod webapi_impl {
    use super::LinkService;

    use application_interface::{
        context::{CurrentUser, WebApiContext},
        error::AppError,
    };
    use application_model::link::models::{
        EditLinkDto, LinkDto, LinkPageListDto, LinkPageRequest, PublicLinkDto,
    };
    use distributed_cache::{CacheEntryOptions, DistributedCacheExt};

    /// WebAPI 宿主友情链接应用服务
    pub struct LinkWebApiService<'a> {
        /// 应用层上下文
        context: WebApiContext<'a>,
    }

    impl<'a> LinkWebApiService<'a> {
        /// 创建 WebAPI 宿主特化友情链接应用服务
        pub fn new(context: WebApiContext<'a>) -> Self {
            Self { context }
        }

        /// 公开友情链接缓存键
        fn public_link_cache_key() -> &'static str {
            "public:link:list"
        }

        /// 获取友情链接分页列表
        pub async fn get_link_list(
            &self,
            params: LinkPageRequest,
        ) -> Result<LinkPageListDto, AppError> {
            LinkService::new(self.context.app)
                .get_link_list(params)
                .await
        }

        /// 获取单个友情链接
        pub async fn get_link(&self, link_id: i64) -> Result<Option<LinkDto>, AppError> {
            LinkService::new(self.context.app).get_link(link_id).await
        }

        /// 获取公开友情链接列表并缓存结果
        pub async fn get_public_link_list(&self) -> Result<Vec<PublicLinkDto>, AppError> {
            if let Some(cache) = self.context.cache {
                if let Some(cached_value) = cache.get_string(Self::public_link_cache_key()).await? {
                    let cached_list = serde_json::from_str::<Vec<PublicLinkDto>>(&cached_value)
                        .map_err(|error| {
                            AppError::internal(format!("反序列化友情链接缓存失败: {error}"))
                        })?;
                    return Ok(cached_list);
                }
            }

            let list = LinkService::new(self.context.app)
                .get_public_link_list()
                .await?;

            if let Some(cache) = self.context.cache {
                let cache_value = serde_json::to_string(&list).map_err(|error| {
                    AppError::internal(format!("序列化友情链接缓存失败: {error}"))
                })?;
                let _ = cache
                    .set_string_with_options(
                        Self::public_link_cache_key(),
                        &cache_value,
                        CacheEntryOptions::relative(std::time::Duration::from_secs(300)),
                    )
                    .await;
            }

            Ok(list)
        }

        /// 创建友情链接并清理公开缓存
        pub async fn create_link(
            &self,
            dto: EditLinkDto,
            current_user: &CurrentUser,
        ) -> Result<String, AppError> {
            let result = LinkService::new(self.context.app)
                .create_link(dto, current_user)
                .await?;
            self.invalidate_public_link_cache().await;
            Ok(result)
        }

        /// 更新友情链接并清理公开缓存
        pub async fn update_link(
            &self,
            link_id: i64,
            dto: EditLinkDto,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let result = LinkService::new(self.context.app)
                .update_link(link_id, dto, current_user)
                .await?;
            self.invalidate_public_link_cache().await;
            Ok(result)
        }

        /// 删除友情链接并清理公开缓存
        pub async fn delete_link(
            &self,
            link_id: i64,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let result = LinkService::new(self.context.app)
                .delete_link(link_id, current_user)
                .await?;
            self.invalidate_public_link_cache().await;
            Ok(result)
        }

        /// 删除公开友情链接缓存
        async fn invalidate_public_link_cache(&self) {
            if let Some(cache) = self.context.cache {
                let _ = cache.remove(Self::public_link_cache_key()).await;
            }
        }
    }
}

pub use webapi_impl::LinkWebApiService;
