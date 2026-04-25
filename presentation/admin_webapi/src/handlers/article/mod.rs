use application_model::article::models::{
    ArticleDto, ArticleIdQuery, ArticlePageListDto, ArticlePageRequest, CategoryDto,
    CategoryIdQuery, CategoryPageListDto, CategoryPageRequest, CategorySelectDto,
    CreateArticleQuery, DeleteArticleQuery, DeleteCategoryQuery, EditArticleDto, EditCategoryDto,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use webapi_core::extract::{Json, PageQuery, Query};
use webapi_core::{auth::UserContext, error::AppError};

use crate::state::AppState;

/// 获取栏目分页列表
#[utoipa::path(
    get,
    path = "/Article/GetCategoryList",
    params(CategoryPageRequest),
    responses(
        (status = 200, description = "查询成功", body = CategoryPageListDto),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn get_category_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<CategoryPageRequest>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();

    match service.get_category_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取栏目树形选择列表
#[utoipa::path(
    get,
    path = "/Article/GetCategorySelectList",
    responses(
        (status = 200, description = "查询成功", body = Vec<CategorySelectDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn get_category_select_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();

    match service.get_category_select_list(None).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取单个栏目详情
#[utoipa::path(
    get,
    path = "/Article/GetCategory",
    params(CategoryIdQuery),
    responses(
        (status = 200, description = "查询成功", body = Option<CategoryDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn get_category(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<CategoryIdQuery>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();

    match service.get_category(query.category_id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建栏目，返回新栏目 ID
#[utoipa::path(
    post,
    path = "/Article/CreateCategory",
    request_body = EditCategoryDto,
    responses(
        (status = 200, description = "创建成功，返回新栏目 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn create_category(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<EditCategoryDto>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();
    let current_user = user_ctx.current_user();

    match service.create_category(dto, &current_user).await {
        Ok(id) => Ok((StatusCode::OK, id).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新栏目信息
#[utoipa::path(
    post,
    path = "/Article/UpdateCategory",
    params(CategoryIdQuery),
    request_body = EditCategoryDto,
    responses(
        (status = 200, description = "更新成功", body = bool),
        (status = 400, description = "请求参数无效或栏目不存在"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn update_category(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<CategoryIdQuery>,
    Json(dto): Json<EditCategoryDto>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();
    let current_user = user_ctx.current_user();

    match service
        .update_category(query.category_id, dto, &current_user)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 软删除栏目
#[utoipa::path(
    delete,
    path = "/Article/DeleteCategory",
    params(DeleteCategoryQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn delete_category(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<DeleteCategoryQuery>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();
    let current_user = user_ctx.current_user();

    match service.delete_category(query.id, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取文章分页列表
#[utoipa::path(
    get,
    path = "/Article/GetArticleList",
    params(ArticlePageRequest),
    responses(
        (status = 200, description = "查询成功", body = ArticlePageListDto),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn get_article_list(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    PageQuery(params): PageQuery<ArticlePageRequest>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();

    match service.get_article_list(params).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取单篇文章详情
#[utoipa::path(
    get,
    path = "/Article/GetArticle",
    params(ArticleIdQuery),
    responses(
        (status = 200, description = "查询成功", body = Option<ArticleDto>),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn get_article(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<ArticleIdQuery>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();

    match service.get_article(query.article_id).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 创建文章，返回新文章 ID
#[utoipa::path(
    post,
    path = "/Article/CreateArticle",
    params(CreateArticleQuery),
    request_body = EditArticleDto,
    responses(
        (status = 200, description = "创建成功，返回新文章 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn create_article(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<CreateArticleQuery>,
    Json(dto): Json<EditArticleDto>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();
    let current_user = user_ctx.current_user();

    match service
        .create_article(dto, query.file_key, &current_user)
        .await
    {
        Ok(id) => Ok((StatusCode::OK, id).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 更新文章信息
#[utoipa::path(
    post,
    path = "/Article/UpdateArticle",
    params(ArticleIdQuery),
    request_body = EditArticleDto,
    responses(
        (status = 200, description = "更新成功", body = bool),
        (status = 400, description = "请求参数无效或文章不存在"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn update_article(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<ArticleIdQuery>,
    Json(dto): Json<EditArticleDto>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();
    let current_user = user_ctx.current_user();

    match service
        .update_article(query.article_id, dto, &current_user)
        .await
    {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 软删除文章
#[utoipa::path(
    delete,
    path = "/Article/DeleteArticle",
    params(DeleteArticleQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文章管理"
)]
pub async fn delete_article(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<DeleteArticleQuery>,
) -> Result<Response, AppError> {
    let service = app_state.article_webapi();
    let current_user = user_ctx.current_user();

    match service.delete_article(query.id, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
