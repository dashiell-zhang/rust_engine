use application_model::region::models::RegionDto;
use async_trait::async_trait;
use common::{app_error::AppError, img_helper};
use distributed_cache::{CacheEntryOptions, DistributedCache, DistributedCacheExt};
use repository_core::read_database::ReadDatabaseConnection;
use std::{collections::HashMap, time::Duration};
use uuid::Uuid;

/// 基础查询只接收读库；缓存仅用于验证码，不缓存地区或字典查询结果。
#[derive(Clone, Copy)]
pub struct BaseContext<'a> {
    pub read_db: &'a ReadDatabaseConnection,
    pub cache: &'a dyn DistributedCache,
}

pub struct BaseService<'a> {
    context: BaseContext<'a>,
}

impl<'a> BaseService<'a> {
    pub fn new(context: BaseContext<'a>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl application_interface::services::base::BaseService for BaseService<'_> {
    async fn get_region(&self, province_id: i32, city_id: i32) -> Result<Vec<RegionDto>, AppError> {
        let service = crate::region::service::RegionService::with_connection(self.context.read_db);
        // 两个条件同时给出时，城市条件决定最终返回的区县列表。
        if city_id != 0 {
            service.get_areas(city_id).await
        } else if province_id != 0 {
            service.get_cities(province_id).await
        } else {
            service.get_provinces().await
        }
    }

    async fn get_region_all(&self) -> Result<Vec<RegionDto>, AppError> {
        crate::region::service::RegionService::with_connection(self.context.read_db)
            .get_region_all()
            .await
    }

    async fn get_value_list(&self, group_id: i64) -> Result<HashMap<String, String>, AppError> {
        crate::app_setting::service::read_value_list(self.context.read_db, group_id).await
    }

    async fn get_verify_code(&self, sign: Uuid) -> Result<Vec<u8>, AppError> {
        let text = rand::random_range(1000..9999u32).to_string();
        let image = img_helper::get_verify_code(&text)
            .map_err(|error| AppError::internal(format!("生成图形验证码失败: {error}")))?;
        self.context
            .cache
            .set_string_with_options(
                &format!("VerifyCode{sign}"),
                &text,
                CacheEntryOptions::relative(Duration::from_secs(300)),
            )
            .await?;
        Ok(image)
    }
}
