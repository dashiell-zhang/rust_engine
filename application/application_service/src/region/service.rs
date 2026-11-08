use application_interface::context::AppContext;
use common::app_error::AppError;
use repository::extensions::SoftDeleteQueryExt;
use repository::{region_area, region_city, region_province, region_town};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};
use std::collections::HashMap;

use super::models::{RegionDto, TownDto};

/// 地区字典应用服务
pub struct RegionService<'a, C = DatabaseConnection> {
    /// 查询连接，可以是主库或只读连接；本服务不执行写入。
    db: &'a C,
}

impl<'a> RegionService<'a> {
    /// 创建地区字典应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { db: context.db }
    }
}

impl<'a, C: ConnectionTrait> RegionService<'a, C> {
    pub fn with_connection(db: &'a C) -> Self {
        Self { db }
    }

    /// 获取全部省份列表
    pub async fn get_provinces(&self) -> Result<Vec<RegionDto>, AppError> {
        let list = region_province::Entity::find()
            .not_deleted(region_province::Column::DeleteTime)
            .order_by_asc(region_province::Column::Id)
            .all(self.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(|m| RegionDto {
                id: m.id.to_string(),
                name: m.province,
                child_list: None,
            })
            .collect::<Vec<_>>();

        Ok(list)
    }

    /// 获取指定省份下的城市列表
    pub async fn get_cities(&self, province_id: i32) -> Result<Vec<RegionDto>, AppError> {
        let list = region_city::Entity::find()
            .filter(region_city::Column::ProvinceId.eq(province_id))
            .not_deleted(region_city::Column::DeleteTime)
            .order_by_asc(region_city::Column::Id)
            .all(self.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(|m| RegionDto {
                id: m.id.to_string(),
                name: m.city,
                child_list: None,
            })
            .collect::<Vec<_>>();

        Ok(list)
    }

    /// 获取指定城市下的区县列表
    pub async fn get_areas(&self, city_id: i32) -> Result<Vec<RegionDto>, AppError> {
        let list = region_area::Entity::find()
            .filter(region_area::Column::CityId.eq(city_id))
            .not_deleted(region_area::Column::DeleteTime)
            .order_by_asc(region_area::Column::Id)
            .all(self.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(|m| RegionDto {
                id: m.id.to_string(),
                name: m.area,
                child_list: None,
            })
            .collect::<Vec<_>>();

        Ok(list)
    }

    /// 获取指定区县下的乡镇列表
    pub async fn get_towns(&self, area_id: i32) -> Result<Vec<TownDto>, AppError> {
        if area_id <= 0 {
            return Err(AppError::bad_request("无效的 areaId"));
        }

        let list = region_town::Entity::find()
            .filter(region_town::Column::AreaId.eq(area_id))
            .not_deleted(region_town::Column::DeleteTime)
            .order_by_asc(region_town::Column::Id)
            .all(self.db)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(|m| TownDto {
                id: m.id,
                name: m.town,
                area_id: m.area_id,
            })
            .collect::<Vec<_>>();

        Ok(list)
    }

    /// 获取完整的省市区树结构
    pub async fn get_region_all(&self) -> Result<Vec<RegionDto>, AppError> {
        // 一次性拉取三层基础数据 在内存中组装树结构 避免前端多次往返请求
        let province_list = region_province::Entity::find()
            .not_deleted(region_province::Column::DeleteTime)
            .order_by_asc(region_province::Column::Id)
            .all(self.db)
            .await
            .map_err(to_app_error)?;

        let city_list = region_city::Entity::find()
            .not_deleted(region_city::Column::DeleteTime)
            .order_by_asc(region_city::Column::Id)
            .all(self.db)
            .await
            .map_err(to_app_error)?;

        let area_list = region_area::Entity::find()
            .not_deleted(region_area::Column::DeleteTime)
            .order_by_asc(region_area::Column::Id)
            .all(self.db)
            .await
            .map_err(to_app_error)?;

        // 先按 city_id 归并区县 方便后续挂载到城市节点
        let mut area_map: HashMap<i32, Vec<RegionDto>> = HashMap::new();
        for area in area_list {
            area_map.entry(area.city_id).or_default().push(RegionDto {
                id: area.id.to_string(),
                name: area.area,
                child_list: None,
            });
        }

        // 再按 province_id 归并城市 并把上一步的区县列表挂到城市节点
        let mut city_map: HashMap<i32, Vec<RegionDto>> = HashMap::new();
        for city in city_list {
            let child_list = area_map.remove(&city.id).unwrap_or_default();
            city_map
                .entry(city.province_id)
                .or_default()
                .push(RegionDto {
                    id: city.id.to_string(),
                    name: city.city,
                    child_list: Some(child_list),
                });
        }

        // 最后按省份顺序输出完整树 结构为 省 -> 市 -> 区
        let mut result = Vec::with_capacity(province_list.len());
        for province in province_list {
            result.push(RegionDto {
                id: province.id.to_string(),
                name: province.province,
                child_list: Some(city_map.remove(&province.id).unwrap_or_default()),
            });
        }

        Ok(result)
    }
}

/// 将数据库错误转换为应用层错误
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}

mod webapi_impl {
    use super::RegionService;

    use application_interface::context::WebApiContext;
    use application_model::region::models::{RegionDto, TownDto};
    use common::app_error::AppError;
    use distributed_cache::{CacheEntryOptions, DistributedCacheExt};

    /// WebAPI 宿主地区字典应用服务
    pub struct RegionWebApiService<'a> {
        /// 应用层上下文
        context: WebApiContext<'a>,
    }

    impl<'a> RegionWebApiService<'a> {
        /// 创建 WebAPI 宿主特化地区字典应用服务
        pub fn new(context: WebApiContext<'a>) -> Self {
            Self { context }
        }

        /// 获取全部省份列表
        pub async fn get_provinces(&self) -> Result<Vec<RegionDto>, AppError> {
            const CACHE_KEY: &str = "region:provinces";

            if let Some(cache) = self.context.cache
                && let Some(cached) = cache.get_string(CACHE_KEY).await?
                && let Ok(list) = serde_json::from_str::<Vec<RegionDto>>(&cached)
            {
                return Ok(list);
            }

            let list = RegionService::new(self.context.app).get_provinces().await?;
            self.try_set_cache(CACHE_KEY, &list).await;

            Ok(list)
        }

        /// 获取指定省份下的城市列表
        pub async fn get_cities(&self, province_id: i32) -> Result<Vec<RegionDto>, AppError> {
            let cache_key = format!("region:cities:{province_id}");

            if let Some(cache) = self.context.cache
                && let Some(cached) = cache.get_string(&cache_key).await?
                && let Ok(list) = serde_json::from_str::<Vec<RegionDto>>(&cached)
            {
                return Ok(list);
            }

            let list = RegionService::new(self.context.app)
                .get_cities(province_id)
                .await?;
            self.try_set_cache(&cache_key, &list).await;

            Ok(list)
        }

        /// 获取指定城市下的区县列表
        pub async fn get_areas(&self, city_id: i32) -> Result<Vec<RegionDto>, AppError> {
            let cache_key = format!("region:areas:{city_id}");

            if let Some(cache) = self.context.cache
                && let Some(cached) = cache.get_string(&cache_key).await?
                && let Ok(list) = serde_json::from_str::<Vec<RegionDto>>(&cached)
            {
                return Ok(list);
            }

            let list = RegionService::new(self.context.app)
                .get_areas(city_id)
                .await?;
            self.try_set_cache(&cache_key, &list).await;

            Ok(list)
        }

        /// 获取指定区县下的乡镇列表
        pub async fn get_towns(&self, area_id: i32) -> Result<Vec<TownDto>, AppError> {
            let cache_key = format!("region:towns:{area_id}");

            if let Some(cache) = self.context.cache
                && let Some(cached) = cache.get_string(&cache_key).await?
                && let Ok(list) = serde_json::from_str::<Vec<TownDto>>(&cached)
            {
                return Ok(list);
            }

            let list = RegionService::new(self.context.app)
                .get_towns(area_id)
                .await?;
            self.try_set_cache(&cache_key, &list).await;

            Ok(list)
        }

        /// 获取完整的省市区树结构
        pub async fn get_region_all(&self) -> Result<Vec<RegionDto>, AppError> {
            const CACHE_KEY: &str = "region:all";

            if let Some(cache) = self.context.cache
                && let Some(cached) = cache.get_string(CACHE_KEY).await?
                && let Ok(list) = serde_json::from_str::<Vec<RegionDto>>(&cached)
            {
                return Ok(list);
            }

            let list = RegionService::new(self.context.app)
                .get_region_all()
                .await?;
            self.try_set_cache(CACHE_KEY, &list).await;

            Ok(list)
        }

        /// 尝试将列表写入缓存
        async fn try_set_cache<T: serde::Serialize>(&self, key: &str, value: &T) {
            if let Some(cache) = self.context.cache
                && let Ok(serialized) = serde_json::to_string(value)
            {
                let _ = cache
                    .set_string_with_options(
                        key,
                        &serialized,
                        CacheEntryOptions::relative(std::time::Duration::from_secs(3600)),
                    )
                    .await;
            }
        }
    }
}

pub use webapi_impl::RegionWebApiService;
