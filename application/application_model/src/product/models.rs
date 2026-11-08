use crate::shared::models::{PageListDto, default_page, default_page_size};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// 商品分页请求参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct ProductPageRequest {
    /// 页码
    #[param(default = default_page)]
    #[serde(default = "default_page")]
    pub page: i32,

    /// 每页条数
    #[param(minimum = 1, maximum = 100, default = default_page_size)]
    #[serde(default = "default_page_size", rename = "pageSize")]
    #[query(validate = "crate::shared::models::validate_page_size_query")]
    pub page_size: i32,

    /// 关键词，匹配商品名称或 SKU
    pub keyword: Option<String>,
}

impl crate::shared::models::PageRequest for ProductPageRequest {
    fn page(&self) -> i32 {
        self.page
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }
}

/// 商品标识参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct ProductIdQuery {
    /// 商品 ID
    #[serde(rename = "productId")]
    pub product_id: i64,
}

/// 删除商品参数
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProductQuery {
    /// 商品 ID
    pub id: i64,
}

/// 商品返回模型
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductDto {
    /// 商品 ID
    pub id: String,

    /// 商品 SKU
    pub sku: String,

    /// 商品名称
    pub name: String,

    /// 商品价格
    pub price: String,

    /// 商品详情
    pub detail: String,

    /// 创建时间
    pub create_time: String,
}

/// 编辑商品请求体
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EditProductDto {
    /// 商品 SKU
    pub sku: String,

    /// 商品名称
    pub name: String,

    /// 商品价格（字符串，保留两位小数）
    pub price: String,

    /// 商品详情
    #[serde(default)]
    pub detail: String,
}

/// 商品分页返回模型
pub type ProductPageListDto = PageListDto<ProductDto>;
