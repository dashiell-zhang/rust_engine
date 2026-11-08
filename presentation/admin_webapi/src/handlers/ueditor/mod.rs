use application_interface::context::CurrentUser;
use application_model::file::models::UploadFileDto;
use axum::{
    extract::{Request, State},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, alphabet, engine::general_purpose};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;
use webapi_core::extract::{Json, Query};
use webapi_core::{auth::UserContext, error::AppError};
use webapi_support::WebApiApplicationExt;

use crate::state::AppState;

mod crawler;
mod form;

/// UEditor 协议处理器
/// 文章上传业务域
const ARTICLE_BUSINESS: &str = "Article";
/// UEditor 默认上传字段名
const DEFAULT_UPLOAD_FIELD_NAME: &str = "upfile";
/// UEditor 图片最大上传大小
const IMAGE_MAX_SIZE: usize = i32::MAX as usize;
/// UEditor 涂鸦最大上传大小
const SCRAWL_MAX_SIZE: usize = i32::MAX as usize;
/// UEditor 文件最大上传大小
const FILE_MAX_SIZE: usize = i32::MAX as usize;
/// UEditor 视频最大上传大小
const VIDEO_MAX_SIZE: usize = i32::MAX as usize;
/// UEditor 图片允许扩展名
const IMAGE_ALLOW_FILES: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".bmp"];
/// UEditor 涂鸦允许扩展名
const SCRAWL_ALLOW_FILES: &[&str] = &[".png"];
/// UEditor 视频允许扩展名
const VIDEO_ALLOW_FILES: &[&str] = &[
    ".flv", ".swf", ".mkv", ".avi", ".rm", ".rmvb", ".mpeg", ".mpg", ".ogg", ".ogv", ".mov",
    ".wmv", ".mp4", ".webm", ".mp3", ".wav", ".mid",
];
/// UEditor 文件允许扩展名
const FILE_ALLOW_FILES: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".flv", ".swf", ".mkv", ".avi", ".rm", ".rmvb",
    ".mpeg", ".mpg", ".ogg", ".ogv", ".mov", ".wmv", ".mp4", ".webm", ".mp3", ".wav", ".mid",
    ".rar", ".zip", ".tar", ".gz", ".7z", ".bz2", ".cab", ".iso", ".doc", ".docx", ".xls", ".xlsx",
    ".ppt", ".pptx", ".pdf", ".txt", ".md", ".xml",
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
#[serde(rename_all = "camelCase")]
#[derive(common::json_helper::input::QueryModel)]
pub struct UeditorActionQuery {
    /// 本次文章编辑上传批次标识
    #[serde(
        default,
        deserialize_with = "common::json_helper::deserialize_optional_query_scalar"
    )]
    pub upload_key: Option<String>,
}

/// UEditor 上传响应
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UeditorUploadResponse {
    /// 协议状态
    state: String,
    /// 资源地址
    url: Option<String>,
    /// 展示标题
    title: Option<String>,
    /// 原始文件名
    original: Option<String>,

    /// 统一文件 ID
    file_id: Option<String>,
    error: Option<String>,
}

/// UEditor 远程抓图单项响应
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UeditorCrawlerItem {
    /// 抓取状态
    state: String,
    /// 源地址
    source: String,
    /// 存储后的访问地址
    url: Option<String>,

    /// 统一文件 ID
    file_id: Option<String>,
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
    // action 来自原始查询集合，按 StringValues 合并，空项不增加逗号。
    let action = common::string_helper::join_string_values(
        serde_urlencoded::from_str::<Vec<(String, String)>>(
            request.uri().query().unwrap_or_default(),
        )
        .unwrap_or_default()
        .into_iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case("action"))
        .map(|(_, value)| value),
    );
    let response = if action == "config" {
        get_ueditor_config(State(app_state)).await.into_response()
    } else {
        upload_ueditor_resource(State(app_state), user_ctx, Query(query), action, request).await?
    };
    // 源 Action 返回包含 JSON 文本的字符串，由字符串格式化器输出。
    Ok(webapi_core::text_response::from_response(response).await)
}

/// 处理 UEditor 上传协议请求
pub async fn upload_ueditor_resource(
    State(app_state): State<Arc<AppState>>,
    user_ctx: UserContext,
    Query(query): Query<UeditorActionQuery>,
    action: String,
    request: Request,
) -> Result<Response, AppError> {
    let current_user = user_ctx.current_user();
    // 源控制器直接比较 Query StringValues，不推断、裁剪或转换 action。
    let Some(upload_key) = query
        .upload_key
        .as_deref()
        .and_then(|value| {
            value
                .trim_matches(|c: char| c.is_ascii_whitespace())
                .parse::<i64>()
                .ok()
        })
        .filter(|value| *value > 0)
    else {
        return Ok(ueditor_state_response("uploadKey 参数为空或格式不正确"));
    };

    match action.as_str() {
        "uploadimage" => {
            handle_multipart_upload_request(
                app_state,
                current_user,
                request,
                upload_key,
                image_upload_spec(),
            )
            .await
        }
        "uploadscrawl" => {
            handle_base64_upload_request(
                app_state,
                current_user,
                request,
                upload_key,
                scrawl_upload_spec(),
            )
            .await
        }
        "uploadvideo" => {
            handle_multipart_upload_request(
                app_state,
                current_user,
                request,
                upload_key,
                video_upload_spec(),
            )
            .await
        }
        "uploadfile" => {
            handle_multipart_upload_request(
                app_state,
                current_user,
                request,
                upload_key,
                file_upload_spec(),
            )
            .await
        }
        "catchimage" => {
            handle_crawl_image_request(app_state, current_user, request, upload_key).await
        }
        _ => Ok(ueditor_state_response(
            "action 参数为空或者 action 不被支持",
        )),
    }
}

/// 处理普通表单上传
async fn handle_multipart_upload_request(
    app_state: Arc<AppState>,
    current_user: CurrentUser,
    request: Request,
    upload_key: i64,
    upload_spec: UeditorUploadSpec,
) -> Result<Response, AppError> {
    let form = match form::read(request, &app_state, Some(upload_spec.field_name)).await {
        Ok(form) => form,
        Err(error) => {
            return Ok(upload_error(
                "文件访问出错，请检查写入权限",
                Some(error),
                None,
            ));
        }
    };
    let Some(file) = form.file.filter(|file| file.length > 0) else {
        return Ok(upload_error(
            "上传请求不正确",
            Some("上传文件为空".into()),
            None,
        ));
    };
    if let Err(message) = validate_ueditor_upload(
        &file.name,
        file.length,
        upload_spec.allow_extensions,
        upload_spec.max_size,
    ) {
        return Ok(upload_error(message, None, None));
    }
    persist_upload_file_from_temp(
        app_state,
        current_user,
        file.name,
        file.temp.path().to_path_buf(),
        upload_key,
        upload_spec.sign,
    )
    .await
}

/// 处理 Base64 涂鸦上传
async fn handle_base64_upload_request(
    app_state: Arc<AppState>,
    current_user: CurrentUser,
    request: Request,
    upload_key: i64,
    upload_spec: UeditorUploadSpec,
) -> Result<Response, AppError> {
    let form = match form::read(request, &app_state, None).await {
        Ok(form) => form,
        Err(error) => {
            return Ok(upload_error(
                "文件访问出错，请检查写入权限",
                Some(error),
                None,
            ));
        }
    };
    let raw_value = common::string_helper::join_string_values(form.values(upload_spec.field_name));
    if raw_value.chars().all(char::is_whitespace) {
        return Ok(upload_error(
            "上传请求不正确",
            Some("上传内容为空".into()),
            None,
        ));
    }
    // Convert.FromBase64String 只忽略空格、Tab、CR、LF，允许未使用的尾位非零。
    let payload: Vec<u8> = raw_value
        .bytes()
        .filter(|value| !matches!(value, b' ' | b'\t' | b'\r' | b'\n'))
        .collect();
    let decoder = general_purpose::GeneralPurpose::new(
        &alphabet::STANDARD,
        general_purpose::GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    );
    let decoded = match decoder.decode(&payload) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Ok(upload_error(
                "上传请求不正确",
                Some("Base64 文件内容格式不正确".into()),
                None,
            ));
        }
    };
    if decoded.len() > upload_spec.max_size {
        return Ok(upload_error("文件大小超出服务器限制", None, None));
    }
    persist_upload_file_from_bytes(
        app_state,
        current_user,
        upload_spec.default_file_name.to_string(),
        &decoded,
        upload_key,
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
    upload_key: i64,
    sign: &str,
) -> Result<Response, AppError> {
    let temp_dir = Path::new(&app_state.settings.file.root_path).join("temps");
    if let Err(error) = fs::create_dir_all(&temp_dir).await {
        return Ok(upload_error(
            "文件访问出错，请检查写入权限",
            Some(error.to_string()),
            None,
        ));
    }
    let temp_file_path = build_temp_upload_path(&temp_dir, &file_name);
    let _temp_guard = common::io_helper::TemporaryFileGuard::new(&temp_file_path);
    if let Err(error) = fs::write(&temp_file_path, bytes).await {
        return Ok(upload_error(
            "文件访问出错，请检查写入权限",
            Some(error.to_string()),
            Some(file_name),
        ));
    }

    persist_upload_file_from_temp(
        app_state,
        current_user,
        file_name,
        temp_file_path,
        upload_key,
        sign,
    )
    .await
}

/// 持久化 UEditor 临时文件
async fn persist_upload_file_from_temp(
    app_state: Arc<AppState>,
    current_user: CurrentUser,
    file_name: String,
    temp_file_path: PathBuf,
    upload_key: i64,
    sign: &str,
) -> Result<Response, AppError> {
    let service = app_state.services().file();
    // 复用现有文件服务 保持上传记录 URL 生成与审计行为一致
    let result = service
        .upload_file(
            UploadFileDto {
                business: ARTICLE_BUSINESS.to_string(),
                key: Some(upload_key),
                sign: sign.to_string(),
                is_public_read: true,
                file_name: file_name.clone(),
                temp_file_path: temp_file_path.to_string_lossy().to_string(),
            },
            &current_user,
        )
        .await;

    cleanup_temp_file(&temp_file_path).await;

    let file_id = match result {
        Ok(file_id) => file_id,
        Err(error) => {
            return Ok(upload_error(
                "文件访问出错，请检查写入权限",
                Some(error.message().to_string()),
                Some(file_name),
            ));
        }
    };
    let file_url = match service.get_file_url(parse_file_id(&file_id)?, false).await {
        Ok(url) => url,
        Err(error) => {
            return Ok(upload_error(
                "文件访问出错，请检查写入权限",
                Some(error.message().to_string()),
                Some(file_name),
            ));
        }
    };
    Ok(Json(UeditorUploadResponse {
        state: "SUCCESS".into(),
        url: Some(normalize_ueditor_url(
            file_url,
            &app_state.settings.file.public_base_url,
        )),
        title: Some(file_name.clone()),
        original: Some(file_name),
        file_id: Some(file_id),
        error: None,
    })
    .into_response())
}

/// 保留协议失败响应的全部字段，区分尚未保存与已进入保存阶段。
fn upload_error(
    state: impl Into<String>,
    error: Option<String>,
    original: Option<String>,
) -> Response {
    Json(UeditorUploadResponse {
        state: state.into(),
        url: None,
        title: original.clone(),
        original,
        file_id: None,
        error,
    })
    .into_response()
}

async fn cleanup_temp_file(path: &Path) {
    let _ = fs::remove_file(path).await;
}

fn build_temp_upload_path(temp_dir: &Path, file_name: &str) -> PathBuf {
    let extension = common::io_helper::file_extension(file_name).to_ascii_lowercase();

    temp_dir.join(format!("{}{}", uuid::Uuid::new_v4(), extension))
}

/// 处理远程抓图
async fn handle_crawl_image_request(
    app_state: Arc<AppState>,
    current_user: CurrentUser,
    request: Request,
    upload_key: i64,
) -> Result<Response, AppError> {
    let form = form::read(request, &app_state, None)
        .await
        .map_err(|error| AppError::from(application_interface::error::AppError::internal(error)))?;
    let sources = form.values("source[]");

    if sources.is_empty() {
        return Ok(Json(json!({
            "state": "参数错误：没有指定抓取源",
        }))
        .into_response());
    }

    let mut items = Vec::with_capacity(sources.len());
    for source in sources {
        items.push(crawler::fetch(&app_state, &current_user, source, upload_key).await);
    }

    Ok(Json(json!({
        "state": "SUCCESS",
        "list": items,
    }))
    .into_response())
}

/// 返回可与 UEditor 配置前缀直接拼接的资源地址。
fn normalize_ueditor_url(url: String, prefix: &str) -> String {
    if prefix.trim().is_empty()
        || !url
            .get(..prefix.len())
            .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
    {
        return url;
    }
    let relative = url[prefix.len()..].trim_start_matches('/');
    if prefix.ends_with('/') {
        relative.to_string()
    } else {
        format!("/{relative}")
    }
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
    let extension = common::io_helper::file_extension(file_name).to_ascii_lowercase();

    if !allow_extensions.contains(&extension.as_str()) {
        return Err("不允许的文件格式".to_string());
    }

    if file_size > max_size {
        return Err("文件大小超出服务器限制".to_string());
    }

    Ok(())
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
        sign: "content-image",
        field_name: DEFAULT_UPLOAD_FIELD_NAME,
        default_file_name: "image.png",
        allow_extensions: IMAGE_ALLOW_FILES,
        max_size: IMAGE_MAX_SIZE,
    }
}

/// 涂鸦上传规格
fn scrawl_upload_spec() -> UeditorUploadSpec {
    UeditorUploadSpec {
        sign: "content-scrawl",
        field_name: DEFAULT_UPLOAD_FIELD_NAME,
        default_file_name: "scrawl.png",
        allow_extensions: SCRAWL_ALLOW_FILES,
        max_size: SCRAWL_MAX_SIZE,
    }
}

/// 视频上传规格
fn video_upload_spec() -> UeditorUploadSpec {
    UeditorUploadSpec {
        sign: "content-video",
        field_name: DEFAULT_UPLOAD_FIELD_NAME,
        default_file_name: "video.mp4",
        allow_extensions: VIDEO_ALLOW_FILES,
        max_size: VIDEO_MAX_SIZE,
    }
}

/// 文件上传规格
fn file_upload_spec() -> UeditorUploadSpec {
    UeditorUploadSpec {
        sign: "content-attachment",
        field_name: DEFAULT_UPLOAD_FIELD_NAME,
        default_file_name: "file.bin",
        allow_extensions: FILE_ALLOW_FILES,
        max_size: FILE_MAX_SIZE,
    }
}

/// 将文件 ID 字符串解析为整数
fn parse_file_id(file_id: &str) -> Result<i64, AppError> {
    file_id.parse::<i64>().map_err(|error| {
        AppError::from(application_interface::error::AppError::internal(format!(
            "解析文件 ID 失败: {error}"
        )))
    })
}
