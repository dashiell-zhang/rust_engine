use application_model::file::models::{
    FileContentDto, FileInfoDto, FileLocalContentDto, ImageFileContentDto, RemoteUploadFileDto,
    UploadFileDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

/// 文件上传、访问与业务关联应用契约。
#[async_trait]
pub trait FileService: Send + Sync {
    async fn upload_file(
        &self,
        dto: UploadFileDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>;
    async fn get_file_list(
        &self,
        business: String,
        sign: Option<String>,
        key: Option<i64>,
        is_get_url: bool,
    ) -> Result<Vec<FileInfoDto>, AppError>;
    async fn get_file_relative_path(&self, file_id: i64) -> Result<String, AppError>;
    async fn bind_files(
        &self,
        business: &str,
        sign: &str,
        upload_key: i64,
        business_id: i64,
        current_user: &CurrentUser,
    ) -> Result<u64, AppError>;
    async fn sync_content_files(
        &self,
        business: &str,
        upload_key: i64,
        business_id: i64,
        referenced_file_ids: &[i64],
        current_user: &CurrentUser,
    ) -> Result<u64, AppError>;
    async fn soft_delete_business_files(
        &self,
        business: &str,
        business_id: i64,
        current_user: &CurrentUser,
    ) -> Result<u64, AppError>;
    async fn get_file_url(&self, file_id: i64, is_inline: bool) -> Result<String, AppError>;
    async fn get_file_content(
        &self,
        file_id: i64,
        allow_private: bool,
    ) -> Result<FileContentDto, AppError>;
    async fn get_file_local_content(
        &self,
        file_id: i64,
        allow_private: bool,
    ) -> Result<FileLocalContentDto, AppError>;
    async fn get_image_content(
        &self,
        file_id: i64,
        width: i32,
        height: i32,
    ) -> Result<ImageFileContentDto, AppError>;
    async fn remote_upload_file(
        &self,
        dto: RemoteUploadFileDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError>;
    async fn delete_file(&self, id: i64, current_user: &CurrentUser) -> Result<bool, AppError>;
}
