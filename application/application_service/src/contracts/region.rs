use application_model::region::models::{RegionDto, TownDto};
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::region::RegionService
    for crate::region::service::RegionWebApiService<'_>
{
    async fn get_provinces(&self) -> Result<Vec<RegionDto>, AppError> {
        crate::region::service::RegionWebApiService::get_provinces(self).await
    }
    async fn get_cities(&self, province_id: i32) -> Result<Vec<RegionDto>, AppError> {
        crate::region::service::RegionWebApiService::get_cities(self, province_id).await
    }
    async fn get_areas(&self, city_id: i32) -> Result<Vec<RegionDto>, AppError> {
        crate::region::service::RegionWebApiService::get_areas(self, city_id).await
    }
    async fn get_towns(&self, area_id: i32) -> Result<Vec<TownDto>, AppError> {
        crate::region::service::RegionWebApiService::get_towns(self, area_id).await
    }
    async fn get_region_all(&self) -> Result<Vec<RegionDto>, AppError> {
        crate::region::service::RegionWebApiService::get_region_all(self).await
    }
}
