use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;
use webapi_core::sign_verify::sign_verify_method_router;

use crate::{
    handlers::base::{get_qr_code, get_region, get_region_all, get_snowflake_id, get_value_list},
    openapi::base::BaseApiDoc,
    state::AppState,
};

use super::WebApiFeature;

/// 基础工具特性
///
/// 提供二维码生成、雪花 ID 获取等工具接口
/// 管理端接口不要求登录鉴权，但需要通过请求签名验证
pub struct BaseFeature;

impl WebApiFeature for BaseFeature {
    /// 注册基础工具路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        // 管理端工具接口无需鉴权 仅挂载签名验证中间件
        router
            .route(
                "/Base/GetRegion",
                sign_verify_method_router(webapi_core::routes::get(get_region), app_state.clone()),
            )
            .route(
                "/Base/GetRegionAll",
                sign_verify_method_router(
                    webapi_core::routes::get(get_region_all),
                    app_state.clone(),
                ),
            )
            .route(
                "/Base/GetQrCode",
                sign_verify_method_router(webapi_core::routes::get(get_qr_code), app_state.clone()),
            )
            .route(
                "/Base/GetValueList",
                sign_verify_method_router(
                    webapi_core::routes::get(get_value_list),
                    app_state.clone(),
                ),
            )
            .route(
                "/Base/GetSnowflakeId",
                sign_verify_method_router(webapi_core::routes::get(get_snowflake_id), app_state),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(BaseApiDoc::openapi())
    }
}
