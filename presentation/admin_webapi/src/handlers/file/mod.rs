use application_model::file::models::UploadFileDto;
use axum::{
    body::Body,
    extract::{Multipart, State},
    http::{
        HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
};
use mime_guess::MimeGuess;
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use utoipa::{IntoParams, ToSchema};
use webapi_core::extract::{Json, Query};
use webapi_core::{auth::UserContext, error::AppError};

use crate::state::AppState;

/// 上传文件查询参数
#[derive(Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadFileQuery {
    /// 业务领域
    pub business: String,

    /// 关联业务主键
    #[serde(rename = "key")]
    pub key: Option<i64>,

    /// 自定义标记
    #[serde(default)]
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
#[serde(rename_all = "camelCase")]
pub struct GetFileUrlQuery {
    /// 文件 ID
    #[serde(rename = "fileId")]
    pub file_id: i64,

    /// 是否内联打开
    #[serde(rename = "isInline", default)]
    pub is_inline: bool,
}

/// 图片缩放查询参数
#[derive(Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetImageQuery {
    /// 文件 ID
    #[serde(rename = "fileId")]
    pub file_id: i64,

    /// 目标宽度
    #[serde(default)]
    pub width: u32,

    /// 目标高度
    #[serde(default)]
    pub height: u32,
}

/// 删除文件查询参数
#[derive(Deserialize, IntoParams, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFileQuery {
    /// 文件 ID
    pub id: i64,
}

/// 接收 multipart 表单并将文件存入文件服务，成功后返回文件 ID
#[utoipa::path(
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
    security(("bearer_auth" = [])),
    tag = "文件管理"
)]
pub async fn upload_file(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<UploadFileQuery>,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let temp_dir = Path::new(&app_state.settings.file.root_path).join("temps");
    fs::create_dir_all(&temp_dir).await.map_err(|error| {
        AppError::from(application_service::error::AppError::internal(format!(
            "创建临时目录失败: {error}"
        )))
    })?;

    let mut file_name = None;
    let mut temp_file_path = None;
    let mut file_length = 0_u64;
    let max_upload_bytes = app_state.settings.file.max_upload_bytes as u64;

    // 与 NetEngine 对齐：直接把上传流复制到临时文件，避免整文件进入内存。
    while let Some(mut field) = multipart.next_field().await.map_err(|error| {
        AppError::from(application_service::error::AppError::bad_request(format!(
            "读取上传文件失败: {error}"
        )))
    })? {
        if field.name() != Some("file") {
            continue;
        }

        let current_file_name = field.file_name().map(str::to_string).ok_or_else(|| {
            AppError::from(application_service::error::AppError::bad_request(
                "缺少上传文件名",
            ))
        })?;
        let temp_path = build_temp_upload_path(&temp_dir, &current_file_name);
        let mut temp_file = fs::File::create(&temp_path).await.map_err(|error| {
            AppError::from(application_service::error::AppError::internal(format!(
                "创建临时文件失败: {error}"
            )))
        })?;
        let mut current_length = 0_u64;

        while let Some(chunk) = field.chunk().await.map_err(|error| {
            AppError::from(application_service::error::AppError::bad_request(format!(
                "读取上传文件内容失败: {error}"
            )))
        })? {
            current_length += chunk.len() as u64;
            if current_length > max_upload_bytes {
                drop(temp_file);
                cleanup_temp_file(&temp_path).await;
                return Err(AppError::from(
                    application_service::error::AppError::bad_request(format!(
                        "上传文件大小超过限制，最大允许 {max_upload_bytes} 字节"
                    )),
                ));
            }

            temp_file.write_all(&chunk).await.map_err(|error| {
                AppError::from(application_service::error::AppError::internal(format!(
                    "写入临时文件失败: {error}"
                )))
            })?;
        }

        temp_file.flush().await.map_err(|error| {
            AppError::from(application_service::error::AppError::internal(format!(
                "刷新临时文件失败: {error}"
            )))
        })?;

        file_name = Some(current_file_name);
        temp_file_path = Some(temp_path);
        file_length = current_length;
        break;
    }

    // 校验必须包含 file 字段且内容不为空
    let file_name = file_name.ok_or_else(|| {
        AppError::from(application_service::error::AppError::bad_request(
            "缺少 file 文件字段",
        ))
    })?;
    let temp_file_path = temp_file_path.ok_or_else(|| {
        AppError::from(application_service::error::AppError::bad_request(
            "缺少 file 文件字段",
        ))
    })?;

    if file_length == 0 {
        cleanup_temp_file(&temp_file_path).await;
        return Err(AppError::from(
            application_service::error::AppError::bad_request("请勿上传空文件"),
        ));
    }

    let temp_file_path_text = temp_file_path.to_string_lossy().to_string();

    let service = app_state.file();
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

    match result {
        Ok(id) => Ok((StatusCode::OK, id).into_response()),
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

/// 获取文件访问地址
#[utoipa::path(
    get,
    path = "/File/GetFileUrl",
    params(GetFileUrlQuery),
    responses(
        (status = 200, description = "查询成功", body = String),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文件管理"
)]
pub async fn get_file_url(
    State(app_state): State<Arc<AppState>>,
    _user_ctx: UserContext,
    Query(query): Query<GetFileUrlQuery>,
) -> Result<Response, AppError> {
    let service = app_state.file();

    match service.get_file_url(query.file_id, query.is_inline).await {
        Ok(result) => Ok((StatusCode::OK, result).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 获取受保护文件内容
#[utoipa::path(
    get,
    path = "/File/GetFile",
    params(GetFileUrlQuery),
    responses(
        (status = 200, description = "读取成功"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文件管理"
)]
pub async fn get_file(
    State(app_state): State<Arc<AppState>>,
    Query(query): Query<GetFileUrlQuery>,
) -> Result<Response, AppError> {
    let service = app_state.file();

    match service.get_file_local_content(query.file_id, true).await {
        Ok(result) => {
            build_physical_file_response(
                result.file_name,
                result.path,
                result.local_path,
                query.is_inline,
            )
            .await
        }
        Err(error) => Err(error.into()),
    }
}

/// 获取按尺寸缩放后的图片内容
///
/// 读取原图后按目标宽高等比缩放，并以内联图片形式返回
#[utoipa::path(
    get,
    path = "/File/GetImage",
    params(GetImageQuery),
    responses(
        (status = 200, description = "读取成功"),
        (status = 400, description = "图片参数无效或文件不是图片"),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文件管理"
)]
pub async fn get_image(
    State(app_state): State<Arc<AppState>>,
    Query(query): Query<GetImageQuery>,
) -> Result<Response, AppError> {
    let service = app_state.file();

    if query.width == 0 && query.height == 0 {
        return match service.get_file_local_content(query.file_id, true).await {
            Ok(result) => {
                build_physical_file_response(result.file_name, result.path, result.local_path, true)
                    .await
            }
            Err(error) => Err(error.into()),
        };
    }

    match service
        .get_image_content(query.file_id, query.width, query.height)
        .await
    {
        Ok(result) => Ok(build_file_response(
            result.file_name,
            result.path,
            Body::from(result.bytes),
            true,
        )),
        Err(error) => Err(error.into()),
    }
}

/// 删除文件
#[utoipa::path(
    delete,
    path = "/File/DeleteFile",
    params(DeleteFileQuery),
    responses(
        (status = 200, description = "删除成功", body = bool),
        (status = 401, description = "未授权"),
        (status = 500, description = "服务器内部错误"),
    ),
    security(("bearer_auth" = [])),
    tag = "文件管理"
)]
pub async fn delete_file(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<DeleteFileQuery>,
) -> Result<Response, AppError> {
    let service = app_state.file();
    let current_user = user_ctx.current_user();

    match service.delete_file(query.id, &current_user).await {
        Ok(result) => Ok((StatusCode::OK, Json(result)).into_response()),
        Err(error) => Err(error.into()),
    }
}

/// 根据本地物理文件构建流式 HTTP 文件响应
///
/// 对齐 NetEngine 的 PhysicalFile 行为，普通文件下载不再整文件读入内存。
async fn build_physical_file_response(
    file_name: String,
    path: String,
    local_path: PathBuf,
    is_inline: bool,
) -> Result<Response, AppError> {
    let file = fs::File::open(&local_path).await.map_err(|error| {
        AppError::from(application_service::error::AppError::internal(format!(
            "打开文件失败: {error}"
        )))
    })?;
    let length = file.metadata().await.ok().map(|metadata| metadata.len());
    let stream = ReaderStream::new(file);
    let mut response = build_file_response(file_name, path, Body::from_stream(stream), is_inline);

    if let Some(length) = length
        && let Ok(value) = HeaderValue::from_str(&length.to_string())
    {
        response
            .headers_mut()
            .insert(axum::http::header::CONTENT_LENGTH, value);
    }

    Ok(response)
}

/// 根据文件内容与元数据构建 HTTP 文件响应
///
/// 依据 path 推断 Content-Type，is_inline 为 true 时设置内联响应头，否则设置下载响应头
fn build_file_response(file_name: String, path: String, body: Body, is_inline: bool) -> Response {
    // 根据文件路径后缀推断 MIME 类型，无法识别时回退到 octet-stream
    let content_type = MimeGuess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    let disposition = if is_inline { "inline" } else { "attachment" };
    let disposition_value = format!("{disposition}; filename=\"{file_name}\"");

    Response::builder()
        .status(StatusCode::OK)
        .header(
            CONTENT_TYPE,
            HeaderValue::from_str(&content_type)
                .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
        )
        .header(
            CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition_value)
                .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
        )
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
