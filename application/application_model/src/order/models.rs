use crate::shared::models::{PageListDto, default_page, default_page_size};
use common::json_helper::deserialize_i64_from_string_or_number;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 订单分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct OrderPageRequest {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 订单号关键词
    pub keyword: Option<String>,

    /// 订单状态筛选
    pub state: Option<String>,

    /// 是否已支付
    pub pay_state: Option<bool>,
}

impl crate::shared::models::PageRequest for OrderPageRequest {
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 订单标识参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct OrderIdQuery {
    /// 订单 ID
    #[serde(rename = "orderId")]
    pub order_id: i64,
}

/// 删除订单参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct DeleteOrderQuery {
    /// 订单 ID
    pub id: i64,
}

/// 更新订单状态请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOrderStatusDto {
    /// 订单 ID
    #[serde(rename = "orderId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    pub order_id: i64,

    /// 目标订单状态
    #[serde(rename = "targetState")]
    pub target_state: String,
}

/// 关闭订单请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloseOrderDto {
    /// 订单 ID
    #[serde(rename = "orderId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    pub order_id: i64,

    /// 关闭原因
    pub reason: Option<String>,
}

/// 退款标记请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundOrderDto {
    /// 订单 ID
    #[serde(rename = "orderId")]
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    pub order_id: i64,

    /// 退款原因
    pub reason: Option<String>,
}

/// 订单返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderDto {
    /// 订单 ID
    pub id: String,

    /// 订单号
    pub order_no: String,

    /// 订单类型
    pub order_type: String,

    /// 订单金额
    pub price: String,

    /// 外部流水号
    pub serial_no: Option<String>,

    /// 订单状态
    pub state: String,

    /// 支付方式
    pub pay_type: String,

    /// 是否已支付
    pub pay_state: bool,

    /// 支付时间
    pub pay_time: Option<String>,

    /// 实付金额
    pub pay_price: Option<String>,

    /// 创建时间
    pub create_time: String,
}

/// 订单明细返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderDetailDto {
    /// 明细 ID
    pub id: String,

    /// 商品 ID
    pub product_id: String,

    /// 商品名称
    pub product_name: String,

    /// 商品 SKU
    pub product_sku: String,

    /// 购买数量
    pub number: i32,
}

/// 订单及明细聚合返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrderWithDetailsDto {
    /// 订单信息
    #[serde(flatten)]
    pub order: OrderDto,

    /// 订单明细列表
    pub details: Vec<OrderDetailDto>,
}

/// 订单分页返回模型
pub type OrderPageListDto = PageListDto<OrderDto>;
