use application_model::region::models::{RegionDto, TownDto};
use async_trait::async_trait;
use common::app_error::AppError;

/// 地区字典应用契约。
#[async_trait]
pub trait RegionService: Send + Sync {
    async fn get_provinces(&self) -> Result<Vec<RegionDto>, AppError>;
    async fn get_cities(&self, province_id: i32) -> Result<Vec<RegionDto>, AppError>;
    async fn get_areas(&self, city_id: i32) -> Result<Vec<RegionDto>, AppError>;
    async fn get_towns(&self, area_id: i32) -> Result<Vec<TownDto>, AppError>;
    async fn get_region_all(&self) -> Result<Vec<RegionDto>, AppError>;
}
