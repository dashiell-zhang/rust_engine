use application_model::file::models::{FileInfoDto, RemoteUploadFileDto};
use utoipa::OpenApi;

/// 文件管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    components(
        schemas(
            FileInfoDto,
            RemoteUploadFileDto,
            file::UploadFileFormData,
        )
    ),
    tags(
        (name = "文件管理", description = "文件上传、查询、读取和删除接口"),
    )
)]
pub struct FileApiDoc;
use crate::handlers::file;
