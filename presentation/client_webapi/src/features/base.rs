use axum::Router;
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};

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
    /// 注册基础工具路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        _app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        // 工具类接口属于公开基础能力 不走鉴权保护
        router
            .route("/Base/GetRegion", axum::routing::get(get_region))
            .route("/Base/GetRegionAll", axum::routing::get(get_region_all))
            .route("/Base/GetQrCode", axum::routing::get(get_qr_code))
            .route("/Base/GetVerifyCode", axum::routing::get(get_verify_code))
            .route("/Base/GetValueList", axum::routing::get(get_value_list))
            .route("/Base/GetSnowflakeId", axum::routing::get(get_snowflake_id))
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(BaseApiDoc::openapi())
    }
}
