use repository::extensions::SoftDeleteQueryExt;
use repository::product;
use repository_core::pagination::SignedPage;
use repository_core::versioned::VersionedActiveModel;
use sea_orm::entity::prelude::Decimal;
use sea_orm::{ColumnTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, Set};
use serde_json::json;
use std::str::FromStr;

use application_interface::error::AppError;
use application_runtime::context::{AppContext, CurrentUser};

use crate::user::models::PageListDto;
use crate::write_support::{
    apply_soft_delete_audit_fields, apply_update_user_audit_field, build_create_full_audit_fields,
    page_offset, validate_page_request, write_data_update_log,
};

use super::models::{EditProductDto, ProductDto, ProductPageListDto, ProductPageRequest};

/// 商品应用服务
pub struct ProductService<'a> {
    /// 应用层上下文
    context: AppContext<'a>,
}

impl<'a> ProductService<'a> {
    /// 创建商品应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 获取商品分页列表
    pub async fn get_product_list(
        &self,
        params: ProductPageRequest,
    ) -> Result<ProductPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query = product::Entity::find().not_deleted(product::Column::DeleteTime);

        if let Some(keyword) = &params.keyword {
            let kw = format!("%{}%", keyword.trim());
            query = query.filter(
                sea_orm::Condition::any()
                    .add(product::Column::Name.like(kw.clone()))
                    .add(product::Column::Sku.like(kw)),
            );
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
        let list = query
            .order_by_desc(product::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(to_product_dto)
            .collect();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }

    /// 获取单个商品
    pub async fn get_product(&self, product_id: i64) -> Result<Option<ProductDto>, AppError> {
        if product_id <= 0 {
            return Err(AppError::bad_request("无效的 productId"));
        }

        let model = product::Entity::find()
            .filter(product::Column::Id.eq(product_id))
            .not_deleted(product::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        Ok(model.map(to_product_dto))
    }

    /// 创建商品，返回新商品 ID
    pub async fn create_product(
        &self,
        dto: EditProductDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        validate_edit_product(&dto)?;
        let price = parse_price(&dto.price)?;

        let id = self
            .context
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成商品主键失败: {error}")))?;
        let audit = build_create_full_audit_fields(current_user);

        product::ActiveModel {
            row_version: Default::default(),
            id: Set(id),
            sku: Set(dto.sku.trim().to_string()),
            name: Set(dto.name.trim().to_string()),
            price: Set(price),
            detail: Set(dto.detail.trim().to_string()),
            create_time: Set(audit.create_time),
            delete_time: Set(audit.delete_time),
            update_time: Set(audit.update_time),
            create_user_id: Set(audit.create_user_id),
            update_user_id: Set(audit.update_user_id),
            delete_user_id: Set(audit.delete_user_id),
        }
        .insert_versioned(self.context.db)
        .await
        .map_err(|e| AppError::internal(format!("创建商品失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Product",
            id,
            "create",
            None,
            Some(json!({ "id": id, "sku": &dto.sku, "name": &dto.name, "price": &dto.price })),
            Some(current_user),
        )
        .await?;

        Ok(id.to_string())
    }

    /// 更新商品信息
    pub async fn update_product(
        &self,
        product_id: i64,
        dto: EditProductDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        if product_id <= 0 {
            return Err(AppError::bad_request("无效的 productId"));
        }

        validate_edit_product(&dto)?;
        let price = parse_price(&dto.price)?;

        let model = product::Entity::find()
            .filter(product::Column::Id.eq(product_id))
            .not_deleted(product::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Err(AppError::bad_request("商品不存在"));
        };

        let before_log = json!({
            "id": model.id,
            "sku": model.sku,
            "name": model.name,
            "price": model.price.to_string(),
            "detail": model.detail,
        });

        let mut active = model.into_active_model();
        active.sku = Set(dto.sku.trim().to_string());
        active.name = Set(dto.name.trim().to_string());
        active.price = Set(price);
        active.detail = Set(dto.detail.trim().to_string());
        apply_update_user_audit_field(&mut active.update_user_id, current_user);

        let updated = active
            .update_versioned(self.context.db)
            .await
            .map_err(|e| AppError::internal(format!("更新商品失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Product",
            product_id,
            "update",
            Some(before_log),
            Some(json!({
                "id": updated.id,
                "sku": updated.sku,
                "name": updated.name,
                "price": updated.price.to_string(),
                "detail": updated.detail,
            })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 软删除商品
    pub async fn delete_product(
        &self,
        product_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        if product_id <= 0 {
            return Err(AppError::bad_request("无效的 productId"));
        }

        let model = product::Entity::find()
            .filter(product::Column::Id.eq(product_id))
            .not_deleted(product::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(true);
        };

        let before_log = json!({
            "id": model.id,
            "sku": model.sku,
            "name": model.name,
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
            .map_err(|e| AppError::internal(format!("删除商品失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Product",
            product_id,
            "delete",
            Some(before_log),
            Some(json!({ "id": product_id, "delete_user_id": current_user.user_id })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }
}

/// 校验编辑商品请求
fn validate_edit_product(dto: &EditProductDto) -> Result<(), AppError> {
    if dto.sku.trim().is_empty() {
        return Err(AppError::bad_request("商品 SKU 不可以为空"));
    }

    if dto.name.trim().is_empty() {
        return Err(AppError::bad_request("商品名称不可以为空"));
    }

    if dto.price.trim().is_empty() {
        return Err(AppError::bad_request("商品价格不可以为空"));
    }

    Ok(())
}

/// 将价格字符串解析为 Decimal
fn parse_price(price_str: &str) -> Result<Decimal, AppError> {
    Decimal::from_str(price_str.trim())
        .map_err(|_| AppError::bad_request("商品价格格式无效，请输入合法的数字"))
}

/// 将实体模型转换为返回模型
fn to_product_dto(model: product::Model) -> ProductDto {
    ProductDto {
        id: model.id.to_string(),
        sku: model.sku,
        name: model.name,
        price: model.price.to_string(),
        detail: model.detail,
        create_time: model.create_time.to_rfc3339(),
    }
}

/// 将数据库错误转换为应用层错误
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}
