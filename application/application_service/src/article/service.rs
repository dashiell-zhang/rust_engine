use repository::extensions::SoftDeleteQueryExt;
use repository_core::pagination::SignedPage;
use repository_core::versioned::VersionedActiveModel;
use std::cmp::Ordering;
use std::future::Future;
use std::pin::Pin;

use async_recursion::async_recursion;
use repository::{article, category};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, sea_query::Expr,
};
use serde_json::json;

use application_interface::{
    context::{AppContext, CurrentUser},
    error::AppError,
};
use application_model::shared::models::PageRequestDto;

use crate::user::models::PageListDto;
use crate::write_support::{
    apply_soft_delete_audit_fields, build_create_audit_fields, page_offset, validate_page_request,
    write_data_update_log, write_data_update_log_with_connection,
};

use super::models::{
    ArticleClickDto, ArticleDto, ArticlePageListDto, CategoryDto, CategoryPageListDto,
    CategoryPageRequest, CategorySelectDto, EditArticleDto, EditCategoryDto,
    PublicAdjacentArticleDto, PublicArticleDetailDto, PublicArticleListItemDto,
    PublicArticleNavigationDto, PublicArticlePageListDto, PublicArticlePageRequest,
    PublicCategoryDto, PublicRecommendArticleRequest,
};

/// 文章模块应用服务，涵盖栏目与文章两类资源的管理
pub struct ArticleService<'a> {
    /// 应用层上下文
    context: AppContext<'a>,
}

impl<'a> ArticleService<'a> {
    /// 创建文章模块应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 获取栏目分页列表
    pub async fn get_category_list(
        &self,
        params: CategoryPageRequest,
    ) -> Result<CategoryPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let query = category::Entity::find().not_deleted(category::Column::DeleteTime);

        let total = query
            .clone()
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
        let categories = query
            .order_by_desc(category::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        let parent_ids: Vec<i64> = categories
            .iter()
            .filter_map(|item| item.parent_id)
            .collect();
        let parents = if parent_ids.is_empty() {
            vec![]
        } else {
            category::Entity::find()
                .filter(category::Column::Id.is_in(parent_ids))
                .not_deleted(category::Column::DeleteTime)
                .all(self.context.db)
                .await
                .map_err(to_app_error)?
        };

        let list = categories
            .into_iter()
            .map(|item| {
                let parent_name = item
                    .parent_id
                    .and_then(|pid| parents.iter().find(|p| p.id == pid).map(|p| p.name.clone()));
                CategoryDto {
                    id: item.id.to_string(),
                    name: item.name,
                    remark: item.remark,
                    sort: item.sort,
                    parent_id: item.parent_id.map(|pid| pid.to_string()),
                    parent_name,
                    create_time: item.create_time.to_rfc3339(),
                }
            })
            .collect();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }

    /// 递归获取栏目树形选择列表，parent_id 为 None 时返回根节点
    pub fn get_category_select_list<'b>(
        &'b self,
        parent_id: Option<i64>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<CategorySelectDto>, AppError>> + Send + 'b>> {
        Box::pin(async move {
            let mut query = category::Entity::find().not_deleted(category::Column::DeleteTime);

            query = match parent_id {
                Some(pid) => query.filter(category::Column::ParentId.eq(pid)),
                None => query.filter(category::Column::ParentId.is_null()),
            };

            let categories = query
                .order_by_asc(category::Column::Sort)
                .order_by_asc(category::Column::Id)
                .all(self.context.db)
                .await
                .map_err(to_app_error)?;

            let mut list = Vec::with_capacity(categories.len());

            for item in categories {
                let child_list = self.get_category_select_list(Some(item.id)).await?;
                list.push(CategorySelectDto {
                    id: item.id.to_string(),
                    name: item.name,
                    child_list,
                });
            }

            Ok(list)
        })
    }

    /// 获取单个栏目详情
    pub async fn get_category(&self, category_id: i64) -> Result<Option<CategoryDto>, AppError> {
        let model = category::Entity::find()
            .filter(category::Column::Id.eq(category_id))
            .not_deleted(category::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(None);
        };

        let parent_name = if let Some(pid) = model.parent_id {
            category::Entity::find()
                .filter(category::Column::Id.eq(pid))
                .not_deleted(category::Column::DeleteTime)
                .one(self.context.db)
                .await
                .map_err(to_app_error)?
                .map(|p| p.name)
        } else {
            None
        };

        Ok(Some(CategoryDto {
            id: model.id.to_string(),
            name: model.name,
            remark: model.remark,
            sort: model.sort,
            parent_id: model.parent_id.map(|pid| pid.to_string()),
            parent_name,
            create_time: model.create_time.to_rfc3339(),
        }))
    }

    /// 创建栏目，返回新栏目 ID
    pub async fn create_category(
        &self,
        dto: EditCategoryDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        if dto.name.trim().is_empty() {
            return Err(AppError::bad_request("栏目名称不可以为空"));
        }

        validate_category_parent(self.context.db, dto.parent_id, None).await?;

        let id = self
            .context
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成栏目主键失败: {error}")))?;
        let audit_after = json!({
            "id": id,
            "name": dto.name,
            "sort": dto.sort,
            "parent_id": dto.parent_id,
            "remark": dto.remark,
        });
        let audit_fields = build_create_audit_fields(current_user);
        let model = category::ActiveModel {
            row_version: Default::default(),
            id: Set(id),
            name: Set(dto.name),
            sort: Set(dto.sort),
            parent_id: Set(dto.parent_id),
            remark: Set(dto.remark),
            create_time: Set(audit_fields.create_time),
            delete_time: Set(audit_fields.delete_time),
            create_user_id: Set(audit_fields.create_user_id),
            delete_user_id: Set(audit_fields.delete_user_id),
        };

        model
            .insert_versioned(self.context.db)
            .await
            .map_err(|e| AppError::internal(format!("创建栏目失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Category",
            id,
            "create",
            None,
            Some(audit_after),
            Some(current_user),
        )
        .await?;

        Ok(id.to_string())
    }

    /// 更新栏目信息
    pub async fn update_category(
        &self,
        category_id: i64,
        dto: EditCategoryDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        if category_id <= 0 {
            return Err(AppError::bad_request("无效的 categoryId"));
        }

        if dto.name.trim().is_empty() {
            return Err(AppError::bad_request("栏目名称不可以为空"));
        }

        validate_category_parent(self.context.db, dto.parent_id, Some(category_id)).await?;

        let model = category::Entity::find()
            .filter(category::Column::Id.eq(category_id))
            .not_deleted(category::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Err(AppError::bad_request("无效的 categoryId"));
        };

        let before_log = json!({
            "id": model.id,
            "name": model.name,
            "sort": model.sort,
            "parent_id": model.parent_id,
            "remark": model.remark,
        });
        let mut active = model.into_active_model();
        active.name = Set(dto.name);
        active.sort = Set(dto.sort);
        active.parent_id = Set(dto.parent_id);
        active.remark = Set(dto.remark);

        let updated_model = active
            .update_versioned(self.context.db)
            .await
            .map_err(|e| AppError::internal(format!("更新栏目失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Category",
            category_id,
            "update",
            Some(before_log),
            Some(json!({
                "id": updated_model.id,
                "name": updated_model.name,
                "sort": updated_model.sort,
                "parent_id": updated_model.parent_id,
                "remark": updated_model.remark,
            })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 软删除栏目
    pub async fn delete_category(
        &self,
        category_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let model = category::Entity::find()
            .filter(category::Column::Id.eq(category_id))
            .not_deleted(category::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(false);
        };

        // 与源服务一致：仅在栏目存在时检查依赖，不允许留下活跃子栏目或文章。
        let has_child = category::Entity::find()
            .filter(category::Column::ParentId.eq(category_id))
            .not_deleted(category::Column::DeleteTime)
            .count(self.context.db)
            .await
            .map_err(to_app_error)?
            > 0;
        if has_child {
            return Err(AppError::bad_request("当前栏目下存在子栏目，无法删除"));
        }
        let has_article = article::Entity::find()
            .filter(article::Column::CategoryId.eq(category_id))
            .not_deleted(article::Column::DeleteTime)
            .count(self.context.db)
            .await
            .map_err(to_app_error)?
            > 0;
        if has_article {
            return Err(AppError::bad_request("当前栏目下存在文章，无法删除"));
        }

        let before_log = json!({
            "id": model.id,
            "name": model.name,
            "sort": model.sort,
            "parent_id": model.parent_id,
            "remark": model.remark,
        });
        let mut active = model.into_active_model();
        apply_soft_delete_audit_fields(
            &mut active.delete_time,
            &mut active.delete_user_id,
            current_user,
        );

        active
            .update_versioned(self.context.db)
            .await
            .map_err(|e| AppError::internal(format!("删除栏目失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Category",
            category_id,
            "delete",
            Some(before_log),
            Some(json!({
                "id": category_id,
                "delete_user_id": current_user.user_id,
            })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 获取文章分页列表
    pub async fn get_article_list(
        &self,
        params: PageRequestDto,
    ) -> Result<ArticlePageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let query = article::Entity::find().not_deleted(article::Column::DeleteTime);

        let total = query
            .clone()
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
        let articles = query
            .order_by_desc(article::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        let category_ids: Vec<i64> = articles.iter().map(|a| a.category_id).collect();
        let categories = if category_ids.is_empty() {
            vec![]
        } else {
            category::Entity::find()
                .filter(category::Column::Id.is_in(category_ids))
                .not_deleted(category::Column::DeleteTime)
                .all(self.context.db)
                .await
                .map_err(to_app_error)?
        };

        let list = articles
            .into_iter()
            .map(|item| {
                let category_name = categories
                    .iter()
                    .find(|c| c.id == item.category_id)
                    .map(|c| c.name.clone());
                ArticleDto {
                    id: item.id.to_string(),
                    category_id: item.category_id.to_string(),
                    category_name,
                    title: item.title,
                    content: item.content,
                    digest: item.digest,
                    is_recommend: item.is_recommend,
                    is_display: item.is_display,
                    sort: item.sort,
                    click_count: item.click_count,
                    create_time: item.create_time.to_rfc3339(),
                    cover_image_list: None,
                }
            })
            .collect();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }

    /// 获取单篇文章详情
    pub async fn get_article(&self, article_id: i64) -> Result<Option<ArticleDto>, AppError> {
        let model = article::Entity::find()
            .filter(article::Column::Id.eq(article_id))
            .not_deleted(article::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(None);
        };

        let category_name = category::Entity::find()
            .filter(category::Column::Id.eq(model.category_id))
            .not_deleted(category::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .map(|c| c.name);

        Ok(Some(ArticleDto {
            id: model.id.to_string(),
            category_id: model.category_id.to_string(),
            category_name,
            title: model.title,
            content: model.content,
            digest: model.digest,
            is_recommend: model.is_recommend,
            is_display: model.is_display,
            sort: model.sort,
            click_count: model.click_count,
            create_time: model.create_time.to_rfc3339(),
            cover_image_list: None,
        }))
    }

    /// 获取客户端文章分页列表，仅返回公开展示的文章
    pub async fn get_public_article_list(
        &self,
        params: PublicArticlePageRequest,
    ) -> Result<PublicArticlePageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query = article::Entity::find()
            .not_deleted(article::Column::DeleteTime)
            .filter(article::Column::IsDisplay.eq(true));

        if let Some(category_id) = params.category_id {
            if params.include_children {
                let mut category_ids = vec![category_id];
                load_child_category_ids(self.context.db, category_id, &mut category_ids).await?;
                query = query.filter(article::Column::CategoryId.is_in(category_ids));
            } else {
                query = query.filter(article::Column::CategoryId.eq(category_id));
            }
        }

        if let Some(keyword) = params
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            query = query.filter(article::Column::Title.contains(keyword));
        }

        let total = query
            .clone()
            .count(self.context.db)
            .await
            .map_err(to_app_error)?;
        let total = repository_core::pagination::count_i32(total).map_err(to_app_error)?;

        if total == 0 {
            return Ok(PageListDto {
                total: 0,
                list: Some(vec![]),
            });
        }

        let skip = page_offset(params.page, params.page_size);
        let articles = query
            .order_by_desc(article::Column::Sort)
            .order_by_desc(article::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        let category_ids: Vec<i64> = articles.iter().map(|item| item.category_id).collect();
        let categories = load_category_map(self.context.db, &category_ids).await?;

        let list = articles
            .into_iter()
            .map(|item| PublicArticleListItemDto {
                id: item.id.to_string(),
                category_id: item.category_id.to_string(),
                category_name: categories
                    .iter()
                    .find(|c| c.id == item.category_id)
                    .map(|c| c.name.clone()),
                title: item.title,
                digest: item.digest,
                is_recommend: item.is_recommend,
                click_count: item.click_count,
                create_time: item.create_time.to_rfc3339(),
            })
            .collect();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }

    /// 获取客户端文章详情，仅返回公开展示的文章
    pub async fn get_public_article(
        &self,
        article_id: i64,
    ) -> Result<Option<PublicArticleDetailDto>, AppError> {
        if article_id <= 0 {
            return Err(AppError::bad_request("无效的 articleId"));
        }

        let model = article::Entity::find()
            .filter(article::Column::Id.eq(article_id))
            .not_deleted(article::Column::DeleteTime)
            .filter(article::Column::IsDisplay.eq(true))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(None);
        };

        let category_name = category::Entity::find()
            .filter(category::Column::Id.eq(model.category_id))
            .not_deleted(category::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .map(|item| item.name);

        Ok(Some(PublicArticleDetailDto {
            id: model.id.to_string(),
            category_id: model.category_id.to_string(),
            category_name,
            title: model.title,
            content: model.content,
            digest: model.digest,
            is_recommend: model.is_recommend,
            click_count: model.click_count,
            create_time: model.create_time.to_rfc3339(),
        }))
    }

    /// 获取客户端公开栏目树
    pub fn get_public_category_tree<'b>(
        &'b self,
        parent_id: Option<i64>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<PublicCategoryDto>, AppError>> + Send + 'b>> {
        Box::pin(async move {
            let mut query = category::Entity::find().not_deleted(category::Column::DeleteTime);

            query = match parent_id {
                Some(pid) => query.filter(category::Column::ParentId.eq(pid)),
                None => query.filter(category::Column::ParentId.is_null()),
            };

            let categories = query
                .order_by_asc(category::Column::Sort)
                .order_by_asc(category::Column::Id)
                .all(self.context.db)
                .await
                .map_err(to_app_error)?;

            let mut list = Vec::with_capacity(categories.len());

            for item in categories {
                let child_list = self.get_public_category_tree(Some(item.id)).await?;
                list.push(PublicCategoryDto {
                    id: item.id.to_string(),
                    name: item.name,
                    child_list,
                });
            }

            Ok(list)
        })
    }

    /// 获取客户端推荐文章列表
    pub async fn get_public_recommend_article_list(
        &self,
        params: PublicRecommendArticleRequest,
    ) -> Result<Vec<PublicArticleListItemDto>, AppError> {
        if params.take == 0 {
            return Err(AppError::bad_request("take 必须大于 0"));
        }

        if params.take > 50 {
            return Err(AppError::bad_request("take 不可以大于 50"));
        }

        let articles = article::Entity::find()
            .not_deleted(article::Column::DeleteTime)
            .filter(article::Column::IsDisplay.eq(true))
            .filter(article::Column::IsRecommend.eq(true))
            .order_by_desc(article::Column::Sort)
            .order_by_desc(article::Column::Id)
            .limit(params.take)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        let category_ids: Vec<i64> = articles.iter().map(|item| item.category_id).collect();
        let categories = load_category_map(self.context.db, &category_ids).await?;

        Ok(articles
            .into_iter()
            .map(|item| PublicArticleListItemDto {
                id: item.id.to_string(),
                category_id: item.category_id.to_string(),
                category_name: categories
                    .iter()
                    .find(|c| c.id == item.category_id)
                    .map(|c| c.name.clone()),
                title: item.title,
                digest: item.digest,
                is_recommend: item.is_recommend,
                click_count: item.click_count,
                create_time: item.create_time.to_rfc3339(),
            })
            .collect::<Vec<_>>())
    }

    /// 累加客户端文章点击量
    pub async fn increase_public_article_click_count(
        &self,
        article_id: i64,
    ) -> Result<Option<ArticleClickDto>, AppError> {
        if article_id <= 0 {
            return Err(AppError::bad_request("无效的 articleId"));
        }

        let model = article::Entity::find()
            .filter(article::Column::Id.eq(article_id))
            .not_deleted(article::Column::DeleteTime)
            .filter(article::Column::IsDisplay.eq(true))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        if model.is_none() {
            return Ok(None);
        }

        let update_result = article::Entity::update_many()
            .col_expr(
                article::Column::ClickCount,
                Expr::col(article::Column::ClickCount).add(1),
            )
            .filter(article::Column::Id.eq(article_id))
            .not_deleted(article::Column::DeleteTime)
            .filter(article::Column::IsDisplay.eq(true))
            .exec(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("更新文章点击量失败: {error}")))?;

        if update_result.rows_affected == 0 {
            return Ok(None);
        }

        let updated = article::Entity::find()
            .filter(article::Column::Id.eq(article_id))
            .not_deleted(article::Column::DeleteTime)
            .filter(article::Column::IsDisplay.eq(true))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .ok_or_else(|| AppError::internal("读取文章点击量失败"))?;

        Ok(Some(ArticleClickDto {
            id: updated.id.to_string(),
            click_count: updated.click_count,
        }))
    }

    /// 获取客户端文章上一篇和下一篇
    pub async fn get_public_article_navigation(
        &self,
        article_id: i64,
    ) -> Result<Option<PublicArticleNavigationDto>, AppError> {
        if article_id <= 0 {
            return Err(AppError::bad_request("无效的 articleId"));
        }

        let current = article::Entity::find()
            .filter(article::Column::Id.eq(article_id))
            .not_deleted(article::Column::DeleteTime)
            .filter(article::Column::IsDisplay.eq(true))
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(current) = current else {
            return Ok(None);
        };

        let mut candidates = article::Entity::find()
            .not_deleted(article::Column::DeleteTime)
            .filter(article::Column::IsDisplay.eq(true))
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        candidates.sort_by(compare_public_article_model);

        let ordered = candidates
            .into_iter()
            .map(|item| PublicAdjacentArticleDto {
                id: item.id.to_string(),
                title: item.title,
            })
            .collect::<Vec<_>>();

        let current_index = ordered
            .iter()
            .position(|item| item.id == current.id.to_string());

        let Some(current_index) = current_index else {
            return Ok(None);
        };

        let previous = if current_index > 0 {
            ordered.get(current_index - 1).cloned()
        } else {
            None
        };

        let next = ordered.get(current_index + 1).cloned();

        Ok(Some(PublicArticleNavigationDto { previous, next }))
    }

    /// 创建文章，返回新文章 ID
    pub async fn create_article(
        &self,
        dto: EditArticleDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        self.create_article_with_connection(self.context.db, dto, current_user)
            .await
    }

    /// 使用指定连接创建文章并返回新文章 ID
    async fn create_article_with_connection<C>(
        &self,
        db: &C,
        dto: EditArticleDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>
    where
        C: ConnectionTrait + sea_orm::TransactionTrait,
    {
        if dto.title.trim().is_empty() {
            return Err(AppError::bad_request("文章标题不可以为空"));
        }

        let category_id = dto
            .category_id
            .trim()
            .parse::<i64>()
            .map_err(|error| AppError::internal(format!("文章栏目 ID 转换失败: {error}")))?;

        let digest = resolve_digest(dto.digest.as_deref(), dto.content.as_deref())?;
        let id = self
            .context
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成文章主键失败: {error}")))?;
        let audit_after = json!({
            "id": id,
            "category_id": category_id,
            "title": dto.title,
            "digest": digest,
            "is_recommend": dto.is_recommend,
            "is_display": dto.is_display,
            "sort": dto.sort,
            "click_count": dto.click_count,
        });
        let audit_fields = build_create_audit_fields(current_user);

        let model = article::ActiveModel {
            row_version: Default::default(),
            id: Set(id),
            category_id: Set(category_id),
            title: Set(dto.title),
            content: Set(dto.content),
            digest: Set(digest),
            is_recommend: Set(dto.is_recommend),
            is_display: Set(dto.is_display),
            sort: Set(dto.sort),
            click_count: Set(dto.click_count),
            create_time: Set(audit_fields.create_time),
            delete_time: Set(audit_fields.delete_time),
            create_user_id: Set(audit_fields.create_user_id),
            delete_user_id: Set(audit_fields.delete_user_id),
        };

        model
            .insert_versioned(db)
            .await
            .map_err(|e| AppError::internal(format!("创建文章失败: {e}")))?;

        write_data_update_log_with_connection(
            db,
            self.context.id_generator,
            "Article",
            id,
            "create",
            None,
            Some(audit_after),
            Some(current_user),
        )
        .await?;

        Ok(id.to_string())
    }

    /// 更新文章信息
    pub async fn update_article(
        &self,
        article_id: i64,
        dto: EditArticleDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        self.update_article_with_connection(self.context.db, article_id, dto, current_user)
            .await
    }

    /// 使用指定连接更新文章信息
    async fn update_article_with_connection<C>(
        &self,
        db: &C,
        article_id: i64,
        dto: EditArticleDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>
    where
        C: ConnectionTrait + sea_orm::TransactionTrait,
    {
        if article_id <= 0 {
            return Err(AppError::bad_request("无效的 articleId"));
        }

        if dto.title.trim().is_empty() {
            return Err(AppError::bad_request("文章标题不可以为空"));
        }

        let model = article::Entity::find()
            .filter(article::Column::Id.eq(article_id))
            .not_deleted(article::Column::DeleteTime)
            .one(db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Err(AppError::bad_request("无效的 articleId"));
        };

        let category_id = dto
            .category_id
            .trim()
            .parse::<i64>()
            .map_err(|error| AppError::internal(format!("文章栏目 ID 转换失败: {error}")))?;

        let digest = resolve_digest(dto.digest.as_deref(), dto.content.as_deref())?;

        let before_log = json!({
            "id": model.id,
            "category_id": model.category_id,
            "title": model.title,
            "digest": model.digest,
            "is_recommend": model.is_recommend,
            "is_display": model.is_display,
            "sort": model.sort,
            "click_count": model.click_count,
        });
        let mut active = model.into_active_model();
        active.category_id = Set(category_id);
        active.title = Set(dto.title);
        active.content = Set(dto.content);
        active.digest = Set(digest);
        active.is_recommend = Set(dto.is_recommend);
        active.is_display = Set(dto.is_display);
        active.sort = Set(dto.sort);
        active.click_count = Set(dto.click_count);

        let updated_model = active
            .update_versioned(db)
            .await
            .map_err(|e| AppError::internal(format!("更新文章失败: {e}")))?;

        write_data_update_log_with_connection(
            db,
            self.context.id_generator,
            "Article",
            article_id,
            "update",
            Some(before_log),
            Some(json!({
                "id": updated_model.id,
                "category_id": updated_model.category_id,
                "title": updated_model.title,
                "digest": updated_model.digest,
                "is_recommend": updated_model.is_recommend,
                "is_display": updated_model.is_display,
                "sort": updated_model.sort,
                "click_count": updated_model.click_count,
            })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 软删除文章
    pub async fn delete_article(
        &self,
        article_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        self.delete_article_with_connection(self.context.db, article_id, current_user)
            .await?;
        Ok(true)
    }

    /// 使用指定连接软删除文章
    async fn delete_article_with_connection<C>(
        &self,
        db: &C,
        article_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError>
    where
        C: ConnectionTrait + sea_orm::TransactionTrait,
    {
        let model = article::Entity::find()
            .filter(article::Column::Id.eq(article_id))
            .not_deleted(article::Column::DeleteTime)
            .one(db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(false);
        };

        let before_log = json!({
            "id": model.id,
            "category_id": model.category_id,
            "title": model.title,
            "digest": model.digest,
            "is_recommend": model.is_recommend,
            "is_display": model.is_display,
            "sort": model.sort,
            "click_count": model.click_count,
        });
        let mut active = model.into_active_model();
        apply_soft_delete_audit_fields(
            &mut active.delete_time,
            &mut active.delete_user_id,
            current_user,
        );

        active
            .update_versioned(db)
            .await
            .map_err(|e| AppError::internal(format!("删除文章失败: {e}")))?;

        write_data_update_log_with_connection(
            db,
            self.context.id_generator,
            "Article",
            article_id,
            "delete",
            Some(before_log),
            Some(json!({
                "id": article_id,
                "delete_user_id": current_user.user_id,
            })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }
}

/// 仅在摘要为 null/空字符串且正文非空时自动生成，保留显式摘要的空白。
fn resolve_digest(digest: Option<&str>, content: Option<&str>) -> Result<Option<String>, AppError> {
    if digest.is_some_and(|text| !text.is_empty()) {
        return Ok(digest.map(str::to_owned));
    }
    let Some(content) = content.filter(|text| !text.is_empty()) else {
        return Ok(digest.map(str::to_owned));
    };
    let text = common::string_helper::remove_html(content);
    let units = text.encode_utf16().take(255).collect::<Vec<_>>();
    // 源 string[..255] 按 UTF-16 截断；若截断代理项，Npgsql 严格编码同样会失败。
    String::from_utf16(&units)
        .map(Some)
        .map_err(|_| AppError::internal("文章摘要截断产生了不完整的 UTF-16 字符"))
}

/// 按栏目 ID 批量加载栏目数据
async fn load_category_map(
    db: &sea_orm::DatabaseConnection,
    category_ids: &[i64],
) -> Result<Vec<category::Model>, AppError> {
    if category_ids.is_empty() {
        return Ok(vec![]);
    }

    category::Entity::find()
        .filter(category::Column::Id.is_in(category_ids.iter().copied()))
        .not_deleted(category::Column::DeleteTime)
        .all(db)
        .await
        .map_err(to_app_error)
}

/// 校验父栏目存在、未删除且不会形成循环引用
async fn validate_category_parent(
    db: &sea_orm::DatabaseConnection,
    parent_id: Option<i64>,
    current_category_id: Option<i64>,
) -> Result<(), AppError> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };

    if parent_id <= 0 {
        return Err(AppError::bad_request("无效的 parentId"));
    }

    if current_category_id == Some(parent_id) {
        return Err(AppError::bad_request("父级栏目不可以指向自身"));
    }

    let mut cursor = Some(parent_id);
    let mut visited = Vec::new();

    while let Some(category_id) = cursor {
        if visited.contains(&category_id) {
            return Err(AppError::bad_request("栏目层级存在循环引用"));
        }
        visited.push(category_id);

        if current_category_id == Some(category_id) {
            return Err(AppError::bad_request("父级栏目不可以指向子栏目"));
        }

        let parent = category::Entity::find()
            .filter(category::Column::Id.eq(category_id))
            .not_deleted(category::Column::DeleteTime)
            .one(db)
            .await
            .map_err(to_app_error)?;

        let Some(parent) = parent else {
            return Err(AppError::bad_request("指定的父级栏目不存在或已删除"));
        };

        cursor = parent.parent_id;
    }

    Ok(())
}

/// 递归加载指定栏目下的全部子栏目 ID
#[async_recursion]
async fn load_child_category_ids(
    db: &sea_orm::DatabaseConnection,
    parent_id: i64,
    category_ids: &mut Vec<i64>,
) -> Result<(), AppError> {
    let children = category::Entity::find()
        .filter(category::Column::ParentId.eq(parent_id))
        .not_deleted(category::Column::DeleteTime)
        .all(db)
        .await
        .map_err(to_app_error)?;

    for child in children {
        if !category_ids.contains(&child.id) {
            category_ids.push(child.id);
        }

        load_child_category_ids(db, child.id, category_ids).await?;
    }

    Ok(())
}

/// 比较客户端公开文章排序
fn compare_public_article_model(left: &article::Model, right: &article::Model) -> Ordering {
    right
        .sort
        .cmp(&left.sort)
        .then_with(|| right.id.cmp(&left.id))
}

/// 将数据库错误转换为应用层错误
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}

#[allow(clippy::collapsible_if)]
mod webapi_impl {
    use repository::extensions::SoftDeleteQueryExt;

    use super::ArticleService;

    use std::future::Future;
    use std::pin::Pin;

    use crate::{file::service::FileService, write_support::begin_transaction};
    use application_interface::{
        context::{CurrentUser, WebApiContext},
        error::AppError,
    };
    use application_model::article::models::{
        ArticleClickDto, ArticleDto, ArticlePageListDto, CategoryDto, CategoryPageListDto,
        CategoryPageRequest, CategorySelectDto, EditArticleDto, EditCategoryDto,
        PublicArticleDetailDto, PublicArticleListItemDto, PublicArticleNavigationDto,
        PublicArticlePageListDto, PublicArticlePageRequest, PublicCategoryDto,
        PublicRecommendArticleRequest,
    };
    use application_model::shared::models::PageRequestDto;
    use distributed_cache::{CacheEntryOptions, DistributedCacheExt};
    use repository::article;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    /// WebAPI 宿主文章应用服务
    pub struct ArticleWebApiService<'a> {
        /// 应用层上下文
        context: WebApiContext<'a>,
    }

    impl<'a> ArticleWebApiService<'a> {
        /// 创建 WebAPI 宿主特化文章应用服务
        pub fn new(context: WebApiContext<'a>) -> Self {
            Self { context }
        }

        /// 公开栏目树缓存键
        fn public_category_tree_cache_key() -> &'static str {
            "public:article:category_tree"
        }

        /// 公开推荐文章缓存键
        fn public_recommend_article_cache_key(take: u64) -> String {
            format!("public:article:recommend:{take}")
        }

        /// 公开文章导航缓存键
        fn public_article_navigation_cache_key(article_id: i64) -> String {
            format!("public:article:navigation:{article_id}")
        }

        fn file_service(&self) -> Option<FileService<'a>> {
            self.context.file.map(FileService::new)
        }

        async fn attach_cover_image_list(&self, article: &mut ArticleDto) -> Result<(), AppError> {
            let Some(file_service) = self.file_service() else {
                return Ok(());
            };

            let article_id = article
                .id
                .parse::<i64>()
                .map_err(|error| AppError::internal(format!("文章 ID 解析失败: {error}")))?;

            let cover_image_list = file_service
                .get_file_list(
                    "Article".to_string(),
                    Some("cover".to_string()),
                    Some(article_id),
                    true,
                )
                .await?;

            article.cover_image_list = Some(cover_image_list);
            Ok(())
        }

        async fn attach_cover_image_list_batch(
            &self,
            articles: &mut [ArticleDto],
        ) -> Result<(), AppError> {
            for article in articles {
                self.attach_cover_image_list(article).await?;
            }

            Ok(())
        }

        /// 获取栏目分页列表
        pub async fn get_category_list(
            &self,
            params: CategoryPageRequest,
        ) -> Result<CategoryPageListDto, AppError> {
            ArticleService::new(self.context.app)
                .get_category_list(params)
                .await
        }

        /// 获取栏目树形选择列表
        pub fn get_category_select_list<'b>(
            &'b self,
            parent_id: Option<i64>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<CategorySelectDto>, AppError>> + Send + 'b>>
        {
            Box::pin(async move {
                ArticleService::new(self.context.app)
                    .get_category_select_list(parent_id)
                    .await
            })
        }

        /// 获取单个栏目详情
        pub async fn get_category(
            &self,
            category_id: i64,
        ) -> Result<Option<CategoryDto>, AppError> {
            ArticleService::new(self.context.app)
                .get_category(category_id)
                .await
        }

        /// 创建栏目并清理公开内容缓存
        pub async fn create_category(
            &self,
            dto: EditCategoryDto,
            current_user: &CurrentUser,
        ) -> Result<String, AppError> {
            let result = ArticleService::new(self.context.app)
                .create_category(dto, current_user)
                .await?;
            self.invalidate_public_article_cache().await;
            Ok(result)
        }

        /// 更新栏目并清理公开内容缓存
        pub async fn update_category(
            &self,
            category_id: i64,
            dto: EditCategoryDto,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let result = ArticleService::new(self.context.app)
                .update_category(category_id, dto, current_user)
                .await?;
            self.invalidate_public_article_cache().await;
            Ok(result)
        }

        /// 删除栏目并清理公开内容缓存
        pub async fn delete_category(
            &self,
            category_id: i64,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let result = ArticleService::new(self.context.app)
                .delete_category(category_id, current_user)
                .await?;
            self.invalidate_public_article_cache().await;
            Ok(result)
        }

        /// 获取文章分页列表
        pub async fn get_article_list(
            &self,
            params: PageRequestDto,
        ) -> Result<ArticlePageListDto, AppError> {
            let mut result = ArticleService::new(self.context.app)
                .get_article_list(params)
                .await?;
            if let Some(list) = result.list.as_mut() {
                self.attach_cover_image_list_batch(list).await?;
            }
            Ok(result)
        }

        /// 获取单篇文章详情
        pub async fn get_article(&self, article_id: i64) -> Result<Option<ArticleDto>, AppError> {
            let mut result = ArticleService::new(self.context.app)
                .get_article(article_id)
                .await?;

            if let Some(article) = result.as_mut() {
                self.attach_cover_image_list(article).await?;
            }

            Ok(result)
        }

        /// 创建文章并清理公开内容缓存
        pub async fn create_article(
            &self,
            dto: EditArticleDto,
            upload_key: i64,
            current_user: &CurrentUser,
        ) -> Result<String, AppError> {
            let file_service = self
                .file_service()
                .ok_or_else(|| AppError::internal("文件服务未配置"))?;
            let content_file_id_list = dto.content_file_id_list.clone();
            let tx = begin_transaction(self.context.app.db).await?;
            let result = ArticleService::new(self.context.app)
                .create_article_with_connection(&tx, dto, current_user)
                .await?;
            let article_id = result
                .parse::<i64>()
                .map_err(|error| AppError::internal(format!("文章 ID 解析失败: {error}")))?;

            file_service
                .sync_content_files_with_connection(
                    &tx,
                    "Article",
                    upload_key,
                    article_id,
                    &content_file_id_list,
                    current_user,
                )
                .await?;
            file_service
                .bind_files_with_connection(
                    &tx,
                    "Article",
                    "cover",
                    upload_key,
                    article_id,
                    current_user,
                )
                .await?;
            tx.commit()
                .await
                .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

            self.invalidate_public_article_cache().await;
            Ok(result)
        }

        /// 更新文章并清理公开内容缓存
        pub async fn update_article(
            &self,
            article_id: i64,
            dto: EditArticleDto,
            upload_key: i64,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let file_service = self
                .file_service()
                .ok_or_else(|| AppError::internal("文件服务未配置"))?;
            let content_file_id_list = dto.content_file_id_list.clone();
            let tx = begin_transaction(self.context.app.db).await?;
            let result = ArticleService::new(self.context.app)
                .update_article_with_connection(&tx, article_id, dto, current_user)
                .await?;
            file_service
                .sync_content_files_with_connection(
                    &tx,
                    "Article",
                    upload_key,
                    article_id,
                    &content_file_id_list,
                    current_user,
                )
                .await?;
            file_service
                .bind_files_with_connection(
                    &tx,
                    "Article",
                    "cover",
                    upload_key,
                    article_id,
                    current_user,
                )
                .await?;
            tx.commit()
                .await
                .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;
            self.invalidate_public_article_cache().await;
            Ok(result)
        }

        /// 删除文章并清理公开内容缓存
        pub async fn delete_article(
            &self,
            article_id: i64,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let file_service = self
                .file_service()
                .ok_or_else(|| AppError::internal("文件服务未配置"))?;
            let tx = begin_transaction(self.context.app.db).await?;
            let article_exists = ArticleService::new(self.context.app)
                .delete_article_with_connection(&tx, article_id, current_user)
                .await?;

            if article_exists {
                file_service
                    .soft_delete_business_files_with_connection(
                        &tx,
                        "Article",
                        article_id,
                        current_user,
                    )
                    .await?;
            }

            tx.commit()
                .await
                .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;
            self.invalidate_public_article_cache().await;
            Ok(true)
        }

        /// 获取公开文章分页列表
        pub async fn get_public_article_list(
            &self,
            params: PublicArticlePageRequest,
        ) -> Result<PublicArticlePageListDto, AppError> {
            ArticleService::new(self.context.app)
                .get_public_article_list(params)
                .await
        }

        /// 获取公开文章详情
        pub async fn get_public_article(
            &self,
            article_id: i64,
        ) -> Result<Option<PublicArticleDetailDto>, AppError> {
            ArticleService::new(self.context.app)
                .get_public_article(article_id)
                .await
        }

        /// 获取公开栏目树并缓存根节点结果
        pub fn get_public_category_tree<'b>(
            &'b self,
            parent_id: Option<i64>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<PublicCategoryDto>, AppError>> + Send + 'b>>
        {
            Box::pin(async move {
                if parent_id.is_none() {
                    if let Some(cache) = self.context.cache {
                        if let Some(cached_value) = cache
                            .get_string(Self::public_category_tree_cache_key())
                            .await?
                        {
                            let cached_list =
                                serde_json::from_str::<Vec<PublicCategoryDto>>(&cached_value)
                                    .map_err(|error| {
                                        AppError::internal(format!(
                                            "反序列化公开栏目树缓存失败: {error}"
                                        ))
                                    })?;
                            return Ok(cached_list);
                        }
                    }
                }

                let list = ArticleService::new(self.context.app)
                    .get_public_category_tree(parent_id)
                    .await?;

                if parent_id.is_none() {
                    if let Some(cache) = self.context.cache {
                        let cache_value = serde_json::to_string(&list).map_err(|error| {
                            AppError::internal(format!("序列化公开栏目树缓存失败: {error}"))
                        })?;
                        let _ = cache
                            .set_string_with_options(
                                Self::public_category_tree_cache_key(),
                                &cache_value,
                                CacheEntryOptions::relative(std::time::Duration::from_secs(300)),
                            )
                            .await;
                    }
                }

                Ok(list)
            })
        }

        /// 获取公开推荐文章列表并缓存结果
        pub async fn get_public_recommend_article_list(
            &self,
            params: PublicRecommendArticleRequest,
        ) -> Result<Vec<PublicArticleListItemDto>, AppError> {
            let cache_key = Self::public_recommend_article_cache_key(params.take);

            if let Some(cache) = self.context.cache {
                if let Some(cached_value) = cache.get_string(&cache_key).await? {
                    let cached_list =
                        serde_json::from_str::<Vec<PublicArticleListItemDto>>(&cached_value)
                            .map_err(|error| {
                                AppError::internal(format!("反序列化公开推荐文章缓存失败: {error}"))
                            })?;
                    return Ok(cached_list);
                }
            }

            let list = ArticleService::new(self.context.app)
                .get_public_recommend_article_list(params)
                .await?;

            if let Some(cache) = self.context.cache {
                let cache_value = serde_json::to_string(&list).map_err(|error| {
                    AppError::internal(format!("序列化公开推荐文章缓存失败: {error}"))
                })?;
                let _ = cache
                    .set_string_with_options(
                        &cache_key,
                        &cache_value,
                        CacheEntryOptions::relative(std::time::Duration::from_secs(300)),
                    )
                    .await;
            }

            Ok(list)
        }

        /// 增加公开文章点击量并清理公开内容缓存
        pub async fn increase_public_article_click_count(
            &self,
            article_id: i64,
        ) -> Result<Option<ArticleClickDto>, AppError> {
            let result = ArticleService::new(self.context.app)
                .increase_public_article_click_count(article_id)
                .await?;
            self.invalidate_public_article_cache().await;
            Ok(result)
        }

        /// 获取公开文章上一篇和下一篇并缓存结果
        pub async fn get_public_article_navigation(
            &self,
            article_id: i64,
        ) -> Result<Option<PublicArticleNavigationDto>, AppError> {
            let cache_key = Self::public_article_navigation_cache_key(article_id);

            if let Some(cache) = self.context.cache {
                if let Some(cached_value) = cache.get_string(&cache_key).await? {
                    let cached_navigation =
                        serde_json::from_str::<Option<PublicArticleNavigationDto>>(&cached_value)
                            .map_err(|error| {
                            AppError::internal(format!("反序列化公开文章导航缓存失败: {error}"))
                        })?;
                    return Ok(cached_navigation);
                }
            }

            let result = ArticleService::new(self.context.app)
                .get_public_article_navigation(article_id)
                .await?;

            if let Some(cache) = self.context.cache {
                let cache_value = serde_json::to_string(&result).map_err(|error| {
                    AppError::internal(format!("序列化公开文章导航缓存失败: {error}"))
                })?;
                let _ = cache
                    .set_string_with_options(
                        &cache_key,
                        &cache_value,
                        CacheEntryOptions::relative(std::time::Duration::from_secs(300)),
                    )
                    .await;
            }

            Ok(result)
        }

        /// 清理公开文章相关缓存
        async fn invalidate_public_article_cache(&self) {
            if let Some(cache) = self.context.cache {
                let _ = cache.remove(Self::public_category_tree_cache_key()).await;

                for take in 1..=50 {
                    let _ = cache
                        .remove(&Self::public_recommend_article_cache_key(take))
                        .await;
                }

                if let Ok(article_ids) = article::Entity::find()
                    .not_deleted(article::Column::DeleteTime)
                    .filter(article::Column::IsDisplay.eq(true))
                    .all(self.context.app.db)
                    .await
                    .map(|items| items.into_iter().map(|item| item.id).collect::<Vec<_>>())
                {
                    for article_id in article_ids {
                        let _ = cache
                            .remove(&Self::public_article_navigation_cache_key(article_id))
                            .await;
                    }
                }
            }
        }
    }
}

pub use webapi_impl::ArticleWebApiService;
