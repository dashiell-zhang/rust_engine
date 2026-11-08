use chrono::Utc;
use repository::extensions::SoftDeleteQueryExt;
use repository::{order, order_detail, product};
use repository_core::pagination::SignedPage;
use repository_core::versioned::VersionedActiveModel;
use sea_orm::{
    ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde_json::json;

use application_interface::{
    context::{AppContext, CurrentUser},
    error::AppError,
};

use crate::user::models::PageListDto;
use crate::write_support::{
    apply_soft_delete_audit_fields, apply_update_user_audit_field, page_offset,
    validate_page_request, write_data_update_log,
};

use super::models::{
    CloseOrderDto, OrderDetailDto, OrderDto, OrderPageListDto, OrderPageRequest,
    OrderWithDetailsDto, RefundOrderDto, UpdateOrderStatusDto,
};

const ORDER_STATE_PENDING: &str = "待支付";
const ORDER_STATE_PAID: &str = "已支付";
const ORDER_STATE_SHIPPED: &str = "已发货";
const ORDER_STATE_COMPLETED: &str = "已完成";
const ORDER_STATE_CLOSED: &str = "已关闭";
const ORDER_STATE_REFUNDED: &str = "已退款";

/// 订单管理应用服务
pub struct OrderService<'a> {
    /// 应用层上下文
    context: AppContext<'a>,
}

impl<'a> OrderService<'a> {
    /// 创建订单管理应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 获取订单分页列表
    pub async fn get_order_list(
        &self,
        params: OrderPageRequest,
    ) -> Result<OrderPageListDto, AppError> {
        validate_page_request(params.page, params.page_size)?;

        let mut query = order::Entity::find().not_deleted(order::Column::DeleteTime);

        if let Some(keyword) = &params.keyword {
            let kw = keyword.trim();
            if !kw.is_empty() {
                query = query.filter(order::Column::OrderNo.like(format!("%{kw}%")));
            }
        }

        if let Some(state) = &params.state {
            let state = state.trim();
            if !state.is_empty() {
                query = query.filter(order::Column::State.eq(state));
            }
        }

        if let Some(pay_state) = params.pay_state {
            query = query.filter(order::Column::PayState.eq(pay_state));
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
            .order_by_desc(order::Column::Id)
            .signed_page(skip, params.page_size)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(to_order_dto)
            .collect();

        Ok(PageListDto {
            total,
            list: Some(list),
        })
    }

    /// 获取订单及明细
    pub async fn get_order(&self, order_id: i64) -> Result<Option<OrderWithDetailsDto>, AppError> {
        if order_id <= 0 {
            return Err(AppError::bad_request("无效的 orderId"));
        }

        let order_model = order::Entity::find()
            .filter(order::Column::Id.eq(order_id))
            .not_deleted(order::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(order_model) = order_model else {
            return Ok(None);
        };

        let details = order_detail::Entity::find()
            .filter(order_detail::Column::OrderId.eq(order_id))
            .not_deleted(order_detail::Column::DeleteTime)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        // 批量加载关联商品
        let product_ids: Vec<i64> = details.iter().map(|d| d.product_id).collect();
        let products = product::Entity::find()
            .filter(product::Column::Id.is_in(product_ids))
            .not_deleted(product::Column::DeleteTime)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;

        let detail_dtos = details
            .into_iter()
            .map(|detail| {
                let prod = products.iter().find(|p| p.id == detail.product_id);
                OrderDetailDto {
                    id: detail.id.to_string(),
                    product_id: detail.product_id.to_string(),
                    product_name: prod.map(|p| p.name.clone()).unwrap_or_default(),
                    product_sku: prod.map(|p| p.sku.clone()).unwrap_or_default(),
                    number: detail.number,
                }
            })
            .collect();

        Ok(Some(OrderWithDetailsDto {
            order: to_order_dto(order_model),
            details: detail_dtos,
        }))
    }

    /// 软删除订单
    pub async fn delete_order(
        &self,
        order_id: i64,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        if order_id <= 0 {
            return Err(AppError::bad_request("无效的 orderId"));
        }

        let model = order::Entity::find()
            .filter(order::Column::Id.eq(order_id))
            .not_deleted(order::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?;

        let Some(model) = model else {
            return Ok(true);
        };

        let before_log = json!({
            "id": model.id,
            "order_no": model.order_no,
            "state": model.state,
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
            .map_err(|e| AppError::internal(format!("删除订单失败: {e}")))?;

        write_data_update_log(
            self.context,
            "Order",
            order_id,
            "delete",
            Some(before_log),
            Some(json!({ "id": order_id, "delete_user_id": current_user.user_id })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 更新订单状态流转
    pub async fn update_order_status(
        &self,
        dto: UpdateOrderStatusDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let target_state = dto.target_state.trim();

        if dto.order_id <= 0 {
            return Err(AppError::bad_request("无效的 orderId"));
        }

        if target_state.is_empty() {
            return Err(AppError::bad_request("目标订单状态不可以为空"));
        }

        let model = self.require_order(dto.order_id).await?;

        if model.state == target_state {
            return Ok(true);
        }

        validate_status_transition(&model, target_state)?;

        let before_log = build_order_log(&model, None);
        let should_fill_pay_time = target_state == ORDER_STATE_PAID && model.pay_time.is_none();
        let should_fill_pay_price = target_state == ORDER_STATE_PAID && model.pay_price.is_none();
        let order_price = model.price;
        let mut active = model.into_active_model();
        active.state = Set(target_state.to_string());

        if target_state == ORDER_STATE_PAID {
            active.pay_state = Set(true);

            if should_fill_pay_time {
                active.pay_time = Set(Some(Utc::now().into()));
            }

            if should_fill_pay_price {
                active.pay_price = Set(Some(order_price));
            }
        }

        apply_update_user_audit_field(&mut active.update_user_id, current_user);

        let updated = active
            .update_versioned(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("更新订单状态失败: {error}")))?;

        write_data_update_log(
            self.context,
            "Order",
            updated.id,
            "update_status",
            Some(before_log),
            Some(build_order_log(&updated, None)),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 关闭订单
    pub async fn close_order(
        &self,
        dto: CloseOrderDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        if dto.order_id <= 0 {
            return Err(AppError::bad_request("无效的 orderId"));
        }

        let reason = normalize_reason(dto.reason.as_deref());
        let model = self.require_order(dto.order_id).await?;

        if model.state == ORDER_STATE_CLOSED {
            return Ok(true);
        }

        if model.state != ORDER_STATE_PENDING {
            return Err(AppError::bad_request("当前订单状态不允许关闭"));
        }

        let before_log = build_order_log(&model, reason.as_deref());
        let mut active = model.into_active_model();
        active.state = Set(ORDER_STATE_CLOSED.to_string());
        apply_update_user_audit_field(&mut active.update_user_id, current_user);

        let updated = active
            .update_versioned(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("关闭订单失败: {error}")))?;

        write_data_update_log(
            self.context,
            "Order",
            updated.id,
            "close",
            Some(before_log),
            Some(build_order_log(&updated, reason.as_deref())),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 标记订单退款
    pub async fn refund_order(
        &self,
        dto: RefundOrderDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        if dto.order_id <= 0 {
            return Err(AppError::bad_request("无效的 orderId"));
        }

        let reason = normalize_reason(dto.reason.as_deref());
        let model = self.require_order(dto.order_id).await?;

        if model.state == ORDER_STATE_REFUNDED {
            return Ok(true);
        }

        if !model.pay_state {
            return Err(AppError::bad_request("未支付订单不允许标记退款"));
        }

        if model.state == ORDER_STATE_CLOSED {
            return Err(AppError::bad_request("已关闭订单不允许标记退款"));
        }

        let before_log = build_order_log(&model, reason.as_deref());
        let mut active = model.into_active_model();
        active.state = Set(ORDER_STATE_REFUNDED.to_string());
        apply_update_user_audit_field(&mut active.update_user_id, current_user);

        let updated = active
            .update_versioned(self.context.db)
            .await
            .map_err(|error| AppError::internal(format!("标记订单退款失败: {error}")))?;

        write_data_update_log(
            self.context,
            "Order",
            updated.id,
            "refund",
            Some(before_log),
            Some(build_order_log(&updated, reason.as_deref())),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 加载指定订单
    async fn require_order(&self, order_id: i64) -> Result<order::Model, AppError> {
        order::Entity::find()
            .filter(order::Column::Id.eq(order_id))
            .not_deleted(order::Column::DeleteTime)
            .one(self.context.db)
            .await
            .map_err(to_app_error)?
            .ok_or_else(|| AppError::bad_request("订单不存在"))
    }
}

/// 校验订单状态流转是否合法
fn validate_status_transition(model: &order::Model, target_state: &str) -> Result<(), AppError> {
    let is_valid = matches!(
        (model.state.as_str(), target_state),
        (ORDER_STATE_PENDING, ORDER_STATE_PAID)
            | (ORDER_STATE_PAID, ORDER_STATE_SHIPPED)
            | (ORDER_STATE_SHIPPED, ORDER_STATE_COMPLETED)
    );

    if !is_valid {
        return Err(AppError::bad_request("当前订单状态不允许流转到目标状态"));
    }

    if target_state != ORDER_STATE_PAID && !model.pay_state {
        return Err(AppError::bad_request("未支付订单不允许继续流转"));
    }

    Ok(())
}

/// 规范化订单操作原因
fn normalize_reason(reason: Option<&str>) -> Option<String> {
    reason
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

/// 构建订单审计日志快照
fn build_order_log(model: &order::Model, reason: Option<&str>) -> serde_json::Value {
    json!({
        "id": model.id,
        "order_no": model.order_no,
        "state": model.state,
        "pay_state": model.pay_state,
        "pay_type": model.pay_type,
        "serial_no": model.serial_no,
        "price": model.price.to_string(),
        "pay_price": model.pay_price.map(|item| item.to_string()),
        "pay_time": model.pay_time.map(|item| item.to_rfc3339()),
        "update_time": model.update_time.map(|item| item.to_rfc3339()),
        "reason": reason,
    })
}

/// 将订单实体转换为返回模型
fn to_order_dto(model: order::Model) -> OrderDto {
    OrderDto {
        id: model.id.to_string(),
        order_no: model.order_no,
        order_type: model.r#type,
        price: model.price.to_string(),
        serial_no: model.serial_no,
        state: model.state,
        pay_type: model.pay_type,
        pay_state: model.pay_state,
        pay_time: model.pay_time.map(|t| t.to_rfc3339()),
        pay_price: model.pay_price.map(|p| p.to_string()),
        create_time: model.create_time.to_rfc3339(),
    }
}

/// 将数据库错误转换为应用层错误
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}
