use crate::{
    features::protect_method_router,
    handlers::article::{
        create_article, create_category, delete_article, delete_category, get_article,
        get_article_list, get_category, get_category_list, get_category_select_list,
        update_article, update_category,
    },
    openapi::article::ArticleApiDoc,
    state::AppState,
};
use std::sync::Arc;
use utoipa::{OpenApi, openapi::OpenApi as OpenApiDocument};
use webapi_core::routes::WebApiRoutes as Router;

use super::{WebApiFeature, WebApiRoutePermission};

/// 文章管理特性
pub struct ArticleFeature;

impl WebApiFeature for ArticleFeature {
    fn validate_resources(
        &self,
        resources: &webapi_core::host::resources::WebApiResources,
    ) -> Result<(), String> {
        webapi_support::services::require::<webapi_support::services::Article>(resources)?;
        Ok(())
    }
    /// 注册当前特性的路由
    fn register(
        &self,
        router: Router<Arc<AppState>>,
        app_state: Arc<AppState>,
    ) -> Router<Arc<AppState>> {
        let permission_module = app_state.settings.permission_module.clone();

        router
            .route(
                "/Article/GetCategoryList",
                protect_method_router(
                    webapi_core::routes::get(get_category_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/GetCategoryList",
                    false,
                ),
            )
            .route(
                "/Article/GetCategorySelectList",
                protect_method_router(
                    webapi_core::routes::get(get_category_select_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/GetCategorySelectList",
                    false,
                ),
            )
            .route(
                "/Article/GetCategory",
                protect_method_router(
                    webapi_core::routes::get(get_category),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/GetCategory",
                    false,
                ),
            )
            .route(
                "/Article/CreateCategory",
                protect_method_router(
                    webapi_core::routes::post(create_category).json_body("createCategory"),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/CreateCategory",
                    false,
                ),
            )
            .route(
                "/Article/UpdateCategory",
                protect_method_router(
                    webapi_core::routes::post(update_category).json_body("updateCategory"),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/UpdateCategory",
                    false,
                ),
            )
            .route(
                "/Article/DeleteCategory",
                protect_method_router(
                    webapi_core::routes::delete(delete_category),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/DeleteCategory",
                    false,
                ),
            )
            .route(
                "/Article/GetArticleList",
                protect_method_router(
                    webapi_core::routes::get(get_article_list),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/GetArticleList",
                    false,
                ),
            )
            .route(
                "/Article/GetArticle",
                protect_method_router(
                    webapi_core::routes::get(get_article),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/GetArticle",
                    false,
                ),
            )
            .route(
                "/Article/CreateArticle",
                protect_method_router(
                    webapi_core::routes::post(create_article).json_body("createArticle"),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/CreateArticle",
                    false,
                ),
            )
            .route(
                "/Article/UpdateArticle",
                protect_method_router(
                    webapi_core::routes::post(update_article).json_body("updateArticle"),
                    app_state.clone(),
                    permission_module.clone(),
                    "/Article/UpdateArticle",
                    false,
                ),
            )
            .route(
                "/Article/DeleteArticle",
                protect_method_router(
                    webapi_core::routes::delete(delete_article),
                    app_state,
                    permission_module,
                    "/Article/DeleteArticle",
                    false,
                ),
            )
    }

    /// 获取当前特性的 OpenAPI 子文档
    fn openapi(&self) -> Option<OpenApiDocument> {
        Some(ArticleApiDoc::openapi())
    }

    /// 获取当前特性的权限路由列表
    fn permission_routes(&self) -> Vec<WebApiRoutePermission> {
        vec![
            WebApiRoutePermission {
                route: "/Article/GetCategoryList",
                remark: "获取栏目列表",
            },
            WebApiRoutePermission {
                route: "/Article/GetCategorySelectList",
                remark: "获取栏目树形列表",
            },
            WebApiRoutePermission {
                route: "/Article/GetCategory",
                remark: "通过栏目Id 获取栏目信息",
            },
            WebApiRoutePermission {
                route: "/Article/CreateCategory",
                remark: "创建栏目",
            },
            WebApiRoutePermission {
                route: "/Article/UpdateCategory",
                remark: "更新栏目信息",
            },
            WebApiRoutePermission {
                route: "/Article/DeleteCategory",
                remark: "删除栏目",
            },
            WebApiRoutePermission {
                route: "/Article/GetArticleList",
                remark: "获取文章列表",
            },
            WebApiRoutePermission {
                route: "/Article/GetArticle",
                remark: "通过文章ID 获取文章信息",
            },
            WebApiRoutePermission {
                route: "/Article/CreateArticle",
                remark: "创建文章",
            },
            WebApiRoutePermission {
                route: "/Article/UpdateArticle",
                remark: "更新文章信息",
            },
            WebApiRoutePermission {
                route: "/Article/DeleteArticle",
                remark: "删除文章",
            },
        ]
    }
}
