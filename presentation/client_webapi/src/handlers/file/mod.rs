use application_model::file::models::{RemoteUploadFileDto, UploadFileDto};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;
use utoipa::{IntoParams, ToSchema};
use webapi_core::extract::{Json, Query};
use webapi_core::uploaded_file::FormFile;
use webapi_core::{auth::UserContext, error::AppError};
use webapi_support::WebApiApplicationExt;

use crate::state::AppState;

/// 上传文件查询参数
#[derive(Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct UploadFileQuery {
    /// 业务领域
    pub business: String,

    /// 关联业务主键
    #[serde(
        rename = "key",
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub key: Option<i64>,

    /// 自定义标记
    pub sign: String,

    /// 是否允许公开访问
    #[serde(rename = "isPublicRead", default)]
    pub is_public_read: bool,
}

/// 上传文件表单
#[derive(ToSchema)]
pub struct UploadFileFormData {
    /// 文件内容
    #[schema(value_type = String, format = Binary)]
    pub file: String,
}

/// 获取文件地址查询参数
#[derive(Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct GetFileUrlQuery {
    /// 文件 ID
    #[serde(rename = "fileId", default)]
    pub file_id: i64,

    /// 是否内联打开
    #[serde(rename = "isInline", default)]
    pub is_inline: bool,
}

/// 原文件下载参数。
#[derive(Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct GetFileQuery {
    #[serde(default, rename = "fileid")]
    pub file_id: i64,
}

/// 图片缩放查询参数
#[derive(Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct GetImageQuery {
    /// 文件 ID
    #[serde(rename = "fileId", default)]
    pub file_id: i64,

    /// 目标宽度
    #[serde(default)]
    pub width: i32,

    /// 目标高度
    #[serde(default)]
    pub height: i32,
}

/// 删除文件查询参数
#[derive(Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct DeleteFileQuery {
    /// 文件 ID
    #[serde(default)]
    pub id: i64,
}

/// 接收 multipart 表单并将文件存入文件服务，成功后返回文件 ID
#[webapi_core::webapi_endpoint(
    policy = authorized,
    permission = "单文件上传接口",
    post,
    path = "/File/UploadFile",
    params(UploadFileQuery),
    request_body(content = UploadFileFormData, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "上传成功，返回文件 ID", body = String),
        (status = 400, description = "请求参数无效"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "文件管理"
)]
pub async fn upload_file(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<UploadFileQuery>,
    file: FormFile,
) -> Result<Response, AppError> {
    if file.is_empty() {
        return Err(application_interface::error::AppError::bad_request("请勿上传空文件").into());
    }
    let file_name = file.file_name().to_owned();
    let temp_dir = Path::new(&app_state.settings.file.root_path).join("temps");
    fs::create_dir_all(&temp_dir).await.map_err(|error| {
        AppError::from(application_interface::error::AppError::internal(format!(
            "创建临时目录失败: {error}"
        )))
    })?;
    let temp_file_path = build_temp_upload_path(&temp_dir, &file_name);
    let temp_file_guard = common::io_helper::TemporaryFileGuard::new(&temp_file_path);
    let mut writer = fs::File::create(&temp_file_path).await.map_err(|error| {
        AppError::from(application_interface::error::AppError::internal(format!(
            "创建临时文件失败: {error}"
        )))
    })?;
    file.copy_to(&mut writer).await?;
    drop(writer);

    let temp_file_path_text = temp_file_path.to_string_lossy().to_string();

    let service = app_state.services().file()?;
    let current_user = user_ctx.current_user();
    let result = service
        .upload_file(
            UploadFileDto {
                business: query.business,
                key: query.key,
                sign: query.sign,
                is_public_read: query.is_public_read,
                file_name,
                temp_file_path: temp_file_path_text,
            },
            &current_user,
        )
        .await;

    cleanup_temp_file(&temp_file_path).await;
    drop(temp_file_guard);

    match result {
        Ok(id) => Ok((StatusCode::OK, Json(id)).into_response()),
        Err(error) => Err(error.into()),
    }
}

async fn cleanup_temp_file(path: &Path) {
    let _ = fs::remove_file(path).await;
}

fn build_temp_upload_path(temp_dir: &Path, file_name: &str) -> PathBuf {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|item| item.to_str())
        .map(|item| format!(".{item}"))
        .unwrap_or_default();

    temp_dir.join(format!("{}{}", uuid::Uuid::new_v4(), extension))
}

/// 通过远程地址下载并上传文件
///
/// 由客户端提交远程文件地址，服务端下载后按普通文件写入存储
#[webapi_core::webapi_endpoint(
    policy = authorized,
    permission = "远程单文件上传接口",
    json_body = "uploadFile",
    post,
    path = "/File/RemoteUploadFile",
    request_body = RemoteUploadFileDto,
    responses(
        (status = 200, description = "上传成功，返回文件 ID", body = String),
        (status = 400, description = "请求参数无效或远程下载失败"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "文件管理"
)]
pub async fn remote_upload_file(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Json(dto): Json<RemoteUploadFileDto>,
) -> Result<Response, AppError> {
    let service = app_state.services().file()?;
    let current_user = user_ctx.current_user();

    match service.remote_upload_file(dto, &current_user).await {
        Ok(id) => Ok((StatusCode::OK, Json(id)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取文件访问地址
#[webapi_core::webapi_endpoint(
    policy = authorized,
    permission = "通过文件ID获取文件静态访问路径",
    get,
    path = "/File/GetFileUrl",
    params(GetFileUrlQuery),
    responses(
        (status = 200, description = "查询成功", body = String),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "文件管理"
)]
pub async fn get_file_url(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<GetFileUrlQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().file()?;

    match service.get_file_url(query.file_id, query.is_inline).await {
        Ok(result) => {
            Ok((StatusCode::OK, webapi_core::text_response::Text(result)).into_response())
        }
        Err(error) => Err(error.into()),
    }
}

/// 获取文件内容
#[webapi_core::webapi_endpoint(
    policy = anonymous,
    permission = none,
    get,
    path = "/File/GetFile",
    params(GetFileQuery),
    responses(
        (status = 200, description = "读取成功"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "文件管理"
)]
pub async fn get_file(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<GetFileQuery>,
) -> Result<Response, AppError> {
    let file = app_state
        .services()
        .file()?
        .get_file_local_content(query.file_id, true)
        .await?;
    webapi_core::file_response::physical_file(file, &headers, false).await
}

/// 获取图片原文件或指定尺寸的 PNG
///
/// 未指定尺寸或任一目标边超过原图时下载原文件，其他情况返回 PNG
#[webapi_core::webapi_endpoint(
    policy = anonymous,
    permission = none,
    get,
    path = "/File/GetImage",
    params(GetImageQuery),
    responses(
        (status = 200, description = "读取成功"),
        (status = 400, description = "图片参数无效或文件不是图片"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "文件管理"
)]
pub async fn get_image(
    State(app_state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<GetImageQuery>,
) -> Result<Response, AppError> {
    let result = app_state
        .services()
        .file()?
        .get_image_content(query.file_id, query.width, query.height)
        .await?;
    webapi_core::file_response::content_type(&result.file.path, true)?;
    match result.resized_png {
        Some(bytes) => Ok(webapi_core::file_response::png(bytes)),
        None => webapi_core::file_response::physical_file(result.file, &headers, true).await,
    }
}

/// 删除文件
#[webapi_core::webapi_endpoint(
    policy = authorized,
    permission = "通过文件ID删除文件方法",
    delete,
    path = "/File/DeleteFile",
    params(DeleteFileQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    tag = "文件管理"
)]
pub async fn delete_file(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<DeleteFileQuery>,
) -> Result<Response, AppError> {
    let service = app_state.services().file()?;
    let current_user = user_ctx.current_user();

    match service.delete_file(query.id, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}
