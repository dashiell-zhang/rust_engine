use repository::extensions::SoftDeleteQueryExt;
use repository_core::versioned::VersionedActiveModel;
use std::collections::HashMap;

use repository::app_setting;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter,
};
use serde_json::json;

use application_interface::{
    context::{AppContext, CurrentUser},
    error::AppError,
};

use crate::write_support::{
    begin_transaction, build_system_create_audit_fields, write_data_update_log_with_connection,
};

use super::models::{EditSiteDto, SiteDto};

/// 站点配置模块名
const SITE_MODULE: &str = "Site";

/// SEO 关键字配置键，保持与 NetEngine 一致
const SITE_SEO_KEYWORDS_KEY: &str = "SeoKeywords";

/// 旧版 NetEngine 误写的 SEO 关键字配置键
const SITE_SEO_KEYWORDS_LEGACY_KEY: &str = "SeoKeyWords";

/// 站点信息应用服务
pub struct SiteService<'a> {
    /// 应用层上下文
    context: AppContext<'a>,
}

impl<'a> SiteService<'a> {
    /// 创建站点信息应用服务
    pub fn new(context: AppContext<'a>) -> Self {
        Self { context }
    }

    /// 获取站点信息
    pub async fn get_site(&self) -> Result<SiteDto, AppError> {
        let models = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(SITE_MODULE))
            .not_deleted(app_setting::Column::DeleteTime)
            .all(self.context.db)
            .await
            .map_err(to_app_error)?;
        Ok(map_site_dto(&models))
    }

    /// 编辑站点信息
    pub async fn update_site(
        &self,
        dto: EditSiteDto,
        current_user: &CurrentUser,
    ) -> Result<bool, AppError> {
        let before_site = self.get_site_without_cache().await?;
        let values = vec![
            ("WebUrl", normalize_site_setting_value(dto.web_url)),
            (
                "ManagerName",
                normalize_site_setting_value(dto.manager_name),
            ),
            (
                "ManagerAddress",
                normalize_site_setting_value(dto.manager_address),
            ),
            (
                "ManagerPhone",
                normalize_site_setting_value(dto.manager_phone),
            ),
            (
                "ManagerEmail",
                normalize_site_setting_value(dto.manager_email),
            ),
            (
                "RecordNumber",
                normalize_site_setting_value(dto.record_number),
            ),
            ("SeoTitle", normalize_site_setting_value(dto.seo_title)),
            (
                SITE_SEO_KEYWORDS_KEY,
                normalize_site_setting_value(dto.seo_keywords),
            ),
            (
                "SeoDescription",
                normalize_site_setting_value(dto.seo_description),
            ),
            ("FootCode", normalize_site_setting_value(dto.foot_code)),
        ];

        let tx = begin_transaction(self.context.db).await?;
        for (key, value) in &values {
            if let Some(value) = value.as_deref() {
                self.upsert_site_setting(&tx, key, value).await?;
            }
        }

        let after_site = self.get_site_without_cache_by_connection(&tx).await?;
        write_data_update_log_with_connection(
            &tx,
            self.context.id_generator,
            "AppSetting",
            0,
            "update_site",
            Some(json!(before_site)),
            Some(json!(after_site)),
            Some(current_user),
        )
        .await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal(format!("提交事务失败: {error}")))?;

        Ok(true)
    }

    /// 获取不走缓存的站点信息
    async fn get_site_without_cache(&self) -> Result<SiteDto, AppError> {
        self.get_site_without_cache_by_connection(self.context.db)
            .await
    }

    /// 通过指定连接获取不走缓存的站点信息
    async fn get_site_without_cache_by_connection<C>(&self, db: &C) -> Result<SiteDto, AppError>
    where
        C: ConnectionTrait,
    {
        let models = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(SITE_MODULE))
            .not_deleted(app_setting::Column::DeleteTime)
            .all(db)
            .await
            .map_err(to_app_error)?;

        Ok(map_site_dto(&models))
    }

    /// 写入单个站点配置
    async fn upsert_site_setting<C>(&self, db: &C, key: &str, value: &str) -> Result<(), AppError>
    where
        C: ConnectionTrait + sea_orm::TransactionTrait,
    {
        let mut models = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq(SITE_MODULE))
            .filter(app_setting::Column::Key.is_in(site_setting_key_candidates(key)))
            .not_deleted(app_setting::Column::DeleteTime)
            .all(db)
            .await
            .map_err(to_app_error)?;

        let model = models
            .iter()
            .position(|item| item.key == key)
            .map(|index| models.swap_remove(index))
            .or_else(|| models.pop());

        match model {
            Some(model) => {
                let mut active = model.into_active_model();
                active.key = Set(key.to_string());
                active.value = Set(value.to_string());
                active.delete_time = Set(None);
                active
                    .update_versioned(db)
                    .await
                    .map_err(|error| AppError::internal(format!("更新站点配置失败: {error}")))?;
            }
            None => {
                let setting_id = self.context.id_generator.get_id().await.map_err(|error| {
                    AppError::internal(format!("生成站点配置主键失败: {error}"))
                })?;
                // 站点配置属于系统初始化写入 这里统一走系统审计字段
                let audit_fields = build_system_create_audit_fields();

                app_setting::ActiveModel {
                    row_version: Default::default(),
                    id: Set(setting_id),
                    module: Set(SITE_MODULE.to_string()),
                    group_id: Set(None),
                    key: Set(key.to_string()),
                    value: Set(value.to_string()),
                    create_time: Set(audit_fields.create_time),
                    delete_time: Set(audit_fields.delete_time),
                }
                .insert_versioned(db)
                .await
                .map_err(|error| AppError::internal(format!("创建站点配置失败: {error}")))?;
            }
        }

        Ok(())
    }
}

/// 将配置实体映射为站点信息
fn map_site_dto(models: &[app_setting::Model]) -> SiteDto {
    let map = models
        .iter()
        .map(|item| (item.key.as_str(), item.value.clone()))
        .collect::<HashMap<_, _>>();

    SiteDto {
        web_url: map.get("WebUrl").cloned(),
        manager_name: map.get("ManagerName").cloned(),
        manager_address: map.get("ManagerAddress").cloned(),
        manager_phone: map.get("ManagerPhone").cloned(),
        manager_email: map.get("ManagerEmail").cloned(),
        record_number: map.get("RecordNumber").cloned(),
        seo_title: map.get("SeoTitle").cloned(),
        seo_keywords: pick_site_value(
            &map,
            SITE_SEO_KEYWORDS_KEY,
            Some(SITE_SEO_KEYWORDS_LEGACY_KEY),
        ),
        seo_description: map.get("SeoDescription").cloned(),
        foot_code: map.get("FootCode").cloned(),
    }
}

/// 站点配置值规范化
///
/// 与 NetEngine 对齐：`None` 表示跳过更新；空字符串仍然会写入，用于显式清空配置。
fn normalize_site_setting_value(value: Option<String>) -> Option<String> {
    value.map(|item| item.trim().to_string())
}

/// 获取站点配置键候选列表
fn site_setting_key_candidates(key: &str) -> Vec<&str> {
    if key == SITE_SEO_KEYWORDS_KEY {
        vec![SITE_SEO_KEYWORDS_KEY, SITE_SEO_KEYWORDS_LEGACY_KEY]
    } else {
        vec![key]
    }
}

/// 读取站点配置值，优先命中新键，必要时兼容旧键
fn pick_site_value(
    map: &HashMap<&str, String>,
    key: &str,
    fallback_key: Option<&str>,
) -> Option<String> {
    map.get(key)
        .cloned()
        .or_else(|| fallback_key.and_then(|legacy_key| map.get(legacy_key).cloned()))
}

/// 将数据库错误转换为应用层错误
fn to_app_error(error: sea_orm::DbErr) -> AppError {
    AppError::internal(error.to_string())
}

#[allow(clippy::collapsible_if)]
mod webapi_impl {
    use super::SiteService;

    use application_interface::{
        context::{CurrentUser, WebApiContext},
        error::AppError,
    };
    use application_model::site::models::{EditSiteDto, SiteDto};
    use distributed_cache::{CacheEntryOptions, DistributedCacheExt};

    /// WebAPI 宿主站点应用服务
    pub struct SiteWebApiService<'a> {
        /// 应用层上下文
        context: WebApiContext<'a>,
    }

    impl<'a> SiteWebApiService<'a> {
        /// 创建 WebAPI 宿主特化站点应用服务
        pub fn new(context: WebApiContext<'a>) -> Self {
            Self { context }
        }

        /// 站点信息缓存键
        fn site_cache_key() -> &'static str {
            "site:info"
        }

        /// 获取站点信息并缓存结果
        pub async fn get_site(&self) -> Result<SiteDto, AppError> {
            if let Some(cache) = self.context.cache {
                if let Some(cached_value) = cache.get_string(Self::site_cache_key()).await? {
                    let cached_site =
                        serde_json::from_str::<SiteDto>(&cached_value).map_err(|error| {
                            AppError::internal(format!("反序列化站点缓存失败: {error}"))
                        })?;
                    return Ok(cached_site);
                }
            }

            let site = SiteService::new(self.context.app).get_site().await?;

            if let Some(cache) = self.context.cache {
                let cache_value = serde_json::to_string(&site)
                    .map_err(|error| AppError::internal(format!("序列化站点缓存失败: {error}")))?;
                let _ = cache
                    .set_string_with_options(
                        Self::site_cache_key(),
                        &cache_value,
                        CacheEntryOptions::relative(std::time::Duration::from_secs(300)),
                    )
                    .await;
            }

            Ok(site)
        }

        /// 更新站点信息并清理缓存
        pub async fn update_site(
            &self,
            dto: EditSiteDto,
            current_user: &CurrentUser,
        ) -> Result<bool, AppError> {
            let result = SiteService::new(self.context.app)
                .update_site(dto, current_user)
                .await?;

            self.invalidate_site_cache().await;

            Ok(result)
        }

        /// 删除站点缓存
        async fn invalidate_site_cache(&self) {
            if let Some(cache) = self.context.cache {
                let _ = cache.remove(Self::site_cache_key()).await;
            }
        }
    }
}

pub use webapi_impl::SiteWebApiService;
