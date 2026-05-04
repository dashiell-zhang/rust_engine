use application_interface::context::CurrentUser;
use application_model::file::models::{RemoteUploadFileDto, UploadFileDto};
use axum::{
    body::to_bytes,
    extract::{FromRequest, Multipart, Request, State},
    http::header,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use webapi_core::body_limit::DEFAULT_BUFFERED_BODY_LIMIT_BYTES;
use webapi_core::extract::{Json, Query};
use webapi_core::{auth::UserContext, error::AppError};

use crate::state::AppState;

/// UEditor 协议处理器
/// UEditor 上传业务域
const UEDITOR_BUSINESS: &str = "ueditor";
/// UEditor 默认上传字段名
const DEFAULT_UPLOAD_FIELD_NAME: &str = "upfile";
/// UEditor 历史前端在配置缺失时可能退化成 undefined 字段名
const FALLBACK_BASE64_FIELD_NAME: &str = "undefined";
/// UEditor 图片最大上传大小
const IMAGE_MAX_SIZE: usize = 10 * 1024 * 1024;
/// UEditor 涂鸦最大上传大小
const SCRAWL_MAX_SIZE: usize = 5 * 1024 * 1024;
/// UEditor 文件最大上传大小
const FILE_MAX_SIZE: usize = 50 * 1024 * 1024;
/// UEditor 视频最大上传大小
const VIDEO_MAX_SIZE: usize = 200 * 1024 * 1024;
/// UEditor 请求体最大上传大小
pub const MAX_UEDITOR_UPLOAD_BODY_BYTES: usize = VIDEO_MAX_SIZE;
/// UEditor 远程抓图表单最大大小
const CRAWL_FORM_MAX_SIZE: usize = 64 * 1024;
/// UEditor 远程抓图最大数量
const MAX_CRAWL_IMAGE_COUNT: usize = 20;
/// UEditor 图片允许扩展名
const IMAGE_ALLOW_FILES: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp"];
/// UEditor 涂鸦允许扩展名
const SCRAWL_ALLOW_FILES: &[&str] = &[".png"];
/// UEditor 视频允许扩展名
const VIDEO_ALLOW_FILES: &[&str] = &[
    ".flv", ".swf", ".mkv", ".avi", ".rm", ".rmvb", ".mpeg", ".mpg", ".ogg", ".ogv", ".mov",
    ".wmv", ".mp4", ".webm", ".mp3", ".wav", ".mid",
];
/// UEditor 文件允许扩展名
const FILE_ALLOW_FILES: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".rar", ".zip", ".tar", ".gz", ".7z", ".bz2",
    ".cab", ".iso", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".pdf", ".txt", ".md",
    ".xml", ".mp4", ".mp3", ".wav",
];

/// UEditor 上传规格
struct UeditorUploadSpec {
    /// 上传动作标识
    sign: &'static str,
    /// 上传字段名
    field_name: &'static str,
    /// 默认文件名
    default_file_name: &'static str,
    /// 允许扩展名
    allow_extensions: &'static [&'static str],
    /// 最大上传大小
    max_size: usize,
}

/// UEditor 动作查询参数
#[derive(Deserialize)]
pub struct UeditorActionQuery {
    /// UEditor 协议动作名
    pub action: Option<String>,
}

/// UEditor 远程抓图表单
#[derive(Deserialize)]
struct UeditorCrawlForm {
    /// 待抓取的远程图片地址
    #[serde(rename = "source[]", default)]
    source: Vec<String>,
}

/// UEditor 上传响应
#[derive(Serialize)]
struct UeditorUploadResponse {
    /// 协议状态
    state: String,
    /// 资源地址
    url: String,
    /// 展示标题
    title: String,
    /// 原始文件名
    original: String,
}

/// UEditor 远程抓图单项响应
#[derive(Serialize)]
struct UeditorCrawlerItem {
    /// 抓取状态
    state: String,
    /// 源地址
    source: String,
    /// 存储后的访问地址
    url: String,
}

/// 获取 UEditor 配置
pub async fn get_ueditor_config(State(app_state): State<Arc<AppState>>) -> Json<Value> {
    Json(build_ueditor_config(&app_state))
}

/// 处理 UEditor 统一请求
pub async fn process_request(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<UeditorActionQuery>,
    request: Request,
) -> Result<Response, AppError> {
    if matches!(query.action.as_deref(), Some("config")) {
        return Ok(get_ueditor_config(State(app_state)).await.into_response());
    }

    upload_ueditor_resource(State(app_state), user_ctx, Query(query), request).await
}

/// 处理 UEditor 上传协议请求
pub async fn upload_ueditor_resource(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<UeditorActionQuery>,
    request: Request,
) -> Result<Response, AppError> {
    let current_user = user_ctx.current_user();
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    // UEditor 通过 action 参数在单一路由下分发多种协议动作
    let action = query
        .action
        .as_deref()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .or_else(|| infer_ueditor_action(content_type));

    match action {
        Some("uploadimage") => {
            handle_multipart_upload_request(app_state, current_user, request, image_upload_spec())
                .await
        }
        Some("uploadscrawl") => {
            handle_base64_upload_request(app_state, current_user, request, scrawl_upload_spec())
                .await
        }
        Some("uploadvideo") => {
            handle_multipart_upload_request(app_state, current_user, request, video_upload_spec())
                .await
        }
        Some("uploadfile") => {
            handle_multipart_upload_request(app_state, current_user, request, file_upload_spec())
                .await
        }
        Some("crawlimage") | Some("catchimage") => {
            handle_crawl_image_request(app_state, current_user, request).await
        }
        _ => Ok(ueditor_state_response(
            "action 参数为空或者 action 不被支持。",
        )),
    }
}

/// 处理普通表单上传
async fn handle_multipart_upload_request(
    app_state: Arc<AppState>,
    current_user: CurrentUser,
    request: Request,
    upload_spec: UeditorUploadSpec,
) -> Result<Response, AppError> {
    let mut multipart = match Multipart::from_request(request, &app_state).await {
        Ok(multipart) => multipart,
        Err(error) => return Ok(ueditor_state_response(format!("读取上传表单失败: {error}"))),
    };

    let temp_dir = Path::new(&app_state.settings.file.root_path).join("temps");
    if let Err(error) = fs::create_dir_all(&temp_dir).await {
        return Ok(ueditor_state_response(format!("创建临时目录失败: {error}")));
    }

    let mut file_name = None;
    let mut temp_file_path = None;
    let mut file_length = 0_usize;

    // 仅提取 UEditor 约定的 upfile 字段 其他表单字段直接忽略
    while let Some(mut field) = match multipart.next_field().await {
        Ok(field) => field,
        Err(error) => return Ok(ueditor_state_response(format!("读取上传字段失败: {error}"))),
    } {
        if field.name() != Some(upload_spec.field_name) {
            continue;
        }

        let current_file_name = field
            .file_name()
            .map(str::to_string)
            .unwrap_or_else(|| upload_spec.default_file_name.to_string());
        let temp_path = build_temp_upload_path(&temp_dir, &current_file_name);
        let mut temp_file = match fs::File::create(&temp_path).await {
            Ok(temp_file) => temp_file,
            Err(error) => return Ok(ueditor_state_response(format!("创建临时文件失败: {error}"))),
        };
        let mut current_length = 0_usize;

        while let Some(chunk) = match field.chunk().await {
            Ok(chunk) => chunk,
            Err(error) => {
                cleanup_temp_file(&temp_path).await;
                return Ok(ueditor_state_response(format!(
                    "读取上传文件内容失败: {error}"
                )));
            }
        } {
            current_length += chunk.len();
            if current_length > upload_spec.max_size {
                cleanup_temp_file(&temp_path).await;
                return Ok(ueditor_state_response("文件大小超出服务器限制"));
            }

            if let Err(error) = temp_file.write_all(&chunk).await {
                cleanup_temp_file(&temp_path).await;
                return Ok(ueditor_state_response(format!("写入临时文件失败: {error}")));
            }
        }

        if let Err(error) = temp_file.flush().await {
            cleanup_temp_file(&temp_path).await;
            return Ok(ueditor_state_response(format!("刷新临时文件失败: {error}")));
        }

        file_name = Some(current_file_name);
        temp_file_path = Some(temp_path);
        file_length = current_length;
        break;
    }

    let file_name = file_name.unwrap_or_else(|| upload_spec.default_file_name.to_string());
    let Some(temp_file_path) = temp_file_path else {
        return Ok(ueditor_state_response(format!(
            "缺少 {} 文件字段",
            upload_spec.field_name
        )));
    };

    if file_length == 0 {
        cleanup_temp_file(&temp_file_path).await;
        return Ok(ueditor_state_response("请勿上传空文件"));
    }

    if let Err(message) = validate_ueditor_upload(
        &file_name,
        file_length,
        upload_spec.allow_extensions,
        upload_spec.max_size,
    ) {
        cleanup_temp_file(&temp_file_path).await;
        return Ok(ueditor_state_response(message));
    }

    persist_upload_file_from_temp(
        app_state,
        current_user,
        file_name,
        temp_file_path,
        upload_spec.sign,
    )
    .await
}

/// 处理 Base64 涂鸦上传
async fn handle_base64_upload_request(
    app_state: Arc<AppState>,
    current_user: CurrentUser,
    request: Request,
    upload_spec: UeditorUploadSpec,
) -> Result<Response, AppError> {
    let bytes = match to_bytes(request.into_body(), DEFAULT_BUFFERED_BODY_LIMIT_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => return Ok(ueditor_state_response(format!("读取上传表单失败: {error}"))),
    };
    let raw_value = serde_urlencoded::from_bytes::<Vec<(String, String)>>(&bytes)
        .ok()
        .and_then(|pairs| {
            pairs.into_iter().find_map(|(key, value)| {
                if key == upload_spec.field_name || key == FALLBACK_BASE64_FIELD_NAME {
                    Some(value)
                } else {
                    None
                }
            })
        });

    let Some(raw_value) = raw_value else {
        return Ok(ueditor_state_response(format!(
            "缺少 {} 文件字段",
            upload_spec.field_name
        )));
    };

    let payload = normalize_base64_payload(&raw_value);
    let decoded = match general_purpose::STANDARD.decode(payload.as_bytes()) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Ok(ueditor_state_response(format!(
                "解析 Base64 图片失败: {error}"
            )));
        }
    };

    if decoded.is_empty() {
        return Ok(ueditor_state_response("请勿上传空文件"));
    }

    if let Err(message) = validate_ueditor_upload(
        upload_spec.default_file_name,
        decoded.len(),
        upload_spec.allow_extensions,
        upload_spec.max_size,
    ) {
        return Ok(ueditor_state_response(message));
    }

    persist_upload_file_from_bytes(
        app_state,
        current_user,
        upload_spec.default_file_name.to_string(),
        decoded.as_slice(),
        upload_spec.sign,
    )
    .await
}

/// 持久化 UEditor 字节上传文件
async fn persist_upload_file_from_bytes(
    app_state: Arc<AppState>,
    current_user: CurrentUser,
    file_name: String,
    bytes: &[u8],
    sign: &str,
) -> Result<Response, AppError> {
    let temp_dir = std::path::Path::new(&app_state.settings.file.root_path).join("temps");
    fs::create_dir_all(&temp_dir).await.map_err(|error| {
        AppError::from(application_service::error::AppError::internal(format!(
            "创建临时目录失败: {error}"
        )))
    })?;
    let temp_file_name = format!("{}_{}", uuid::Uuid::new_v4(), file_name);
    let temp_file_path = temp_dir.join(temp_file_name);
    fs::write(&temp_file_path, &bytes).await.map_err(|error| {
        AppError::from(application_service::error::AppError::internal(format!(
            "写入临时文件失败: {error}"
        )))
    })?;

    persist_upload_file_from_temp(app_state, current_user, file_name, temp_file_path, sign).await
}

/// 持久化 UEditor 临时文件
async fn persist_upload_file_from_temp(
    app_state: Arc<AppState>,
    current_user: CurrentUser,
    file_name: String,
    temp_file_path: PathBuf,
    sign: &str,
) -> Result<Response, AppError> {
    let service = app_state.file();
    // 复用现有文件服务 保持上传记录 URL 生成与审计行为一致
    let result = service
        .upload_file(
            UploadFileDto {
                business: UEDITOR_BUSINESS.to_string(),
                key: None,
                sign: sign.to_string(),
                is_public_read: true,
                file_name: file_name.clone(),
                temp_file_path: temp_file_path.to_string_lossy().to_string(),
            },
            &current_user,
        )
        .await;

    cleanup_temp_file(&temp_file_path).await;

    let file_id = result.map_err(AppError::from)?;
    let file_url = service
        .get_file_relative_path(parse_file_id(&file_id)?)
        .await
        .map_err(AppError::from)?;

    Ok(Json(UeditorUploadResponse {
        state: "SUCCESS".to_string(),
        url: file_url,
        title: file_name.clone(),
        original: file_name,
    })
    .into_response())
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

/// 处理远程抓图
async fn handle_crawl_image_request(
    app_state: Arc<AppState>,
    current_user: CurrentUser,
    request: Request,
) -> Result<Response, AppError> {
    let bytes = to_bytes(request.into_body(), CRAWL_FORM_MAX_SIZE)
        .await
        .map_err(|error| {
            AppError::from(application_service::error::AppError::bad_request(format!(
                "读取抓图请求体失败: {error}"
            )))
        })?;
    let form = serde_urlencoded::from_bytes::<UeditorCrawlForm>(&bytes).map_err(|error| {
        AppError::from(application_service::error::AppError::bad_request(format!(
            "解析抓图参数失败: {error}"
        )))
    })?;

    if form.source.is_empty() {
        return Ok(Json(json!({
            "state": "参数错误：没有指定抓取源",
        }))
        .into_response());
    }

    let service = app_state.file();
    let mut items = Vec::new();

    // 抓图数量做上限裁剪 避免单次请求无限放大后端开销
    for source in form.source.into_iter().take(MAX_CRAWL_IMAGE_COUNT) {
        let result = service
            .remote_upload_file(
                RemoteUploadFileDto {
                    business: UEDITOR_BUSINESS.to_string(),
                    key: None,
                    sign: "catchimage".to_string(),
                    is_public_read: true,
                    file_name: "crawl_image".to_string(),
                    url: source.clone(),
                },
                &current_user,
            )
            .await;

        match result {
            Ok(file_id) => {
                let url = service
                    .get_file_relative_path(parse_file_id(&file_id)?)
                    .await
                    .map_err(AppError::from)?;

                items.push(UeditorCrawlerItem {
                    state: "SUCCESS".to_string(),
                    source,
                    url,
                });
            }
            Err(error) => {
                items.push(UeditorCrawlerItem {
                    state: error.message().to_string(),
                    source,
                    url: String::new(),
                });
            }
        }
    }

    Ok(Json(json!({
        "state": "SUCCESS",
        "list": items,
    }))
    .into_response())
}

/// 构建 UEditor 配置响应
fn build_ueditor_config(app_state: &AppState) -> Value {
    // 这里返回的是 UEditor 约定的配置结构 键名需保持兼容
    json!({
        "imageActionName": "uploadimage",
        "imageFieldName": DEFAULT_UPLOAD_FIELD_NAME,
        "imageMaxSize": IMAGE_MAX_SIZE,
        "imageAllowFiles": IMAGE_ALLOW_FILES,
        "imageCompressEnable": true,
        "imageCompressBorder": 1600,
        "imageInsertAlign": "none",
        "imageUrlPrefix": app_state.settings.file.public_base_url,
        "imagePathFormat": "uploads/{yyyy}/{mm}/{dd}/{time}{rand:6}",
        "scrawlActionName": "uploadscrawl",
        "scrawlFieldName": DEFAULT_UPLOAD_FIELD_NAME,
        "scrawlPathFormat": "uploads/{yyyy}/{mm}/{dd}/{time}{rand:6}",
        "scrawlMaxSize": SCRAWL_MAX_SIZE,
        "scrawlUrlPrefix": app_state.settings.file.public_base_url,
        "scrawlInsertAlign": "none",
        "snapscreenActionName": "uploadimage",
        "snapscreenPathFormat": "uploads/{yyyy}/{mm}/{dd}/{time}{rand:6}",
        "snapscreenUrlPrefix": app_state.settings.file.public_base_url,
        "snapscreenInsertAlign": "none",
        "catcherLocalDomain": ["127.0.0.1", "localhost", "img.baidu.com"],
        "catcherActionName": "catchimage",
        "catcherFieldName": "source",
        "catcherPathFormat": "uploads/{yyyy}/{mm}/{dd}/{time}{rand:6}",
        "catcherUrlPrefix": app_state.settings.file.public_base_url,
        "catcherMaxSize": IMAGE_MAX_SIZE,
        "catcherAllowFiles": IMAGE_ALLOW_FILES,
        "videoActionName": "uploadvideo",
        "videoFieldName": DEFAULT_UPLOAD_FIELD_NAME,
        "videoPathFormat": "uploads/{yyyy}/{mm}/{dd}/{time}{rand:6}",
        "videoUrlPrefix": app_state.settings.file.public_base_url,
        "videoMaxSize": VIDEO_MAX_SIZE,
        "videoAllowFiles": VIDEO_ALLOW_FILES,
        "fileActionName": "uploadfile",
        "fileFieldName": DEFAULT_UPLOAD_FIELD_NAME,
        "filePathFormat": "uploads/{yyyy}/{mm}/{dd}/{time}{rand:6}",
        "fileUrlPrefix": app_state.settings.file.public_base_url,
        "fileMaxSize": FILE_MAX_SIZE,
        "fileAllowFiles": FILE_ALLOW_FILES
    })
}

/// 校验 UEditor 上传文件
fn validate_ueditor_upload(
    file_name: &str,
    file_size: usize,
    allow_extensions: &[&str],
    max_size: usize,
) -> Result<(), String> {
    // 扩展名统一转成小写后再校验 兼容前端上传时的大小写差异
    let extension = Path::new(file_name)
        .extension()
        .and_then(|item| item.to_str())
        .map(|item| format!(".{}", item.to_ascii_lowercase()))
        .ok_or_else(|| "上传文件缺少扩展名".to_string())?;

    if !allow_extensions.contains(&extension.as_str()) {
        return Err("不允许的文件格式".to_string());
    }

    if file_size > max_size {
        return Err("文件大小超出服务器限制".to_string());
    }

    Ok(())
}

/// 根据请求头推断 UEditor 动作
fn infer_ueditor_action(content_type: &str) -> Option<&'static str> {
    if content_type
        .to_ascii_lowercase()
        .starts_with("application/x-www-form-urlencoded")
    {
        return Some("uploadscrawl");
    }

    None
}

/// 归一化 Base64 数据 去掉 data URL 头部
fn normalize_base64_payload(payload: &str) -> &str {
    if let Some((_, value)) = payload.split_once(',')
        && payload.starts_with("data:")
    {
        return value;
    }

    payload
}

/// 构造 UEditor 统一状态响应
fn ueditor_state_response(message: impl Into<String>) -> Response {
    Json(json!({
        "state": message.into(),
    }))
    .into_response()
}

/// 图片上传规格
fn image_upload_spec() -> UeditorUploadSpec {
    UeditorUploadSpec {
        sign: "uploadimage",
        field_name: DEFAULT_UPLOAD_FIELD_NAME,
        default_file_name: "image.png",
        allow_extensions: IMAGE_ALLOW_FILES,
        max_size: IMAGE_MAX_SIZE,
    }
}

/// 涂鸦上传规格
fn scrawl_upload_spec() -> UeditorUploadSpec {
    UeditorUploadSpec {
        sign: "uploadscrawl",
        field_name: DEFAULT_UPLOAD_FIELD_NAME,
        default_file_name: "scrawl.png",
        allow_extensions: SCRAWL_ALLOW_FILES,
        max_size: SCRAWL_MAX_SIZE,
    }
}

/// 视频上传规格
fn video_upload_spec() -> UeditorUploadSpec {
    UeditorUploadSpec {
        sign: "uploadvideo",
        field_name: DEFAULT_UPLOAD_FIELD_NAME,
        default_file_name: "video.mp4",
        allow_extensions: VIDEO_ALLOW_FILES,
        max_size: VIDEO_MAX_SIZE,
    }
}

/// 文件上传规格
fn file_upload_spec() -> UeditorUploadSpec {
    UeditorUploadSpec {
        sign: "uploadfile",
        field_name: DEFAULT_UPLOAD_FIELD_NAME,
        default_file_name: "file.bin",
        allow_extensions: FILE_ALLOW_FILES,
        max_size: FILE_MAX_SIZE,
    }
}

/// 将文件 ID 字符串解析为整数
fn parse_file_id(file_id: &str) -> Result<i64, AppError> {
    file_id.parse::<i64>().map_err(|error| {
        AppError::from(application_service::error::AppError::internal(format!(
            "解析文件 ID 失败: {error}"
        )))
    })
}
