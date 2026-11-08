use application_model::region::models::RegionDto;
use async_trait::async_trait;
use common::app_error::AppError;
use std::collections::HashMap;
use uuid::Uuid;

/// 公共基础数据查询与图形验证码，与两个 WebAPI 共享。
#[async_trait]
pub trait BaseService: Send + Sync {
    async fn get_region(&self, province_id: i32, city_id: i32) -> Result<Vec<RegionDto>, AppError>;
    async fn get_region_all(&self) -> Result<Vec<RegionDto>, AppError>;
    async fn get_value_list(&self, group_id: i64) -> Result<HashMap<String, String>, AppError>;
    async fn get_verify_code(&self, sign: Uuid) -> Result<Vec<u8>, AppError>;
}
