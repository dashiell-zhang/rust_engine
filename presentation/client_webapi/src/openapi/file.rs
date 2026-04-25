use application_model::file::models::{FileInfoDto, RemoteUploadFileDto};
use utoipa::OpenApi;

use crate::handlers::file;

/// 文件管理模块 OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    paths(
        file::upload_file,
        file::remote_upload_file,
        file::get_file_url,
        file::get_file,
        file::get_image,
        file::delete_file,
    ),
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
