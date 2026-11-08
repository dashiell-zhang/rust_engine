use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;

use crate::{
    handlers::base::{
        get_qr_code, get_region, get_region_all, get_snowflake_id, get_value_list, get_verify_code,
    },
    openapi::base::BaseApiDoc,
    state::AppState,
};

use super::WebApiFeature;

/// 基础工具特性
///
/// 提供二维码生成、雪花 ID 获取等无需鉴权的公开工具接口
pub struct BaseFeature;

impl WebApiFeature for BaseFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::Base>(resources)?;
        Ok(())
    }
    /// 注册基础工具路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        _app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        // 工具类接口属于公开基础能力 不走鉴权保护
        router
            .route("/Base/GetRegion", webapi_core::routes::get(get_region))
            .route(
                "/Base/GetRegionAll",
                webapi_core::routes::get(get_region_all),
            )
            .route("/Base/GetQrCode", webapi_core::routes::get(get_qr_code))
            .route(
                "/Base/GetVerifyCode",
                webapi_core::routes::get(get_verify_code),
            )
            .route(
                "/Base/GetValueList",
                webapi_core::routes::get(get_value_list),
            )
            .route(
                "/Base/GetSnowflakeId",
                webapi_core::routes::get(get_snowflake_id),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(BaseApiDoc::openapi())
    }
}
