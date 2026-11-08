use application_model::file::models::{
    FileContentDto, FileInfoDto, FileLocalContentDto, ImageFileContentDto, RemoteUploadFileDto,
    UploadFileDto,
};
use application_model::shared::current_user::CurrentUser;
use async_trait::async_trait;
use common::app_error::AppError;

#[async_trait]
impl application_interface::services::file::FileService for crate::file::service::FileService<'_> {
    async fn upload_file(
        &self,
        dto: UploadFileDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        crate::file::service::FileService::upload_file(self, dto, current_user).await
    }
    async fn get_file_list(
        &self,
        business: String,
        sign: Option<String>,
        key: Option<i64>,
        is_get_url: bool,
    ) -> Result<Vec<FileInfoDto>, AppError> {
        crate::file::service::FileService::get_file_list(self, business, sign, key, is_get_url)
            .await
    }
    async fn get_file_relative_path(&self, file_id: i64) -> Result<String, AppError> {
        crate::file::service::FileService::get_file_relative_path(self, file_id).await
    }
    async fn bind_files(
        &self,
        business: &str,
        sign: &str,
        upload_key: i64,
        business_id: i64,
        current_user: &CurrentUser,
    ) -> Result<u64, AppError> {
        crate::file::service::FileService::bind_files(
            self,
            business,
            sign,
            upload_key,
            business_id,
            current_user,
        )
        .await
    }
    async fn sync_content_files(
        &self,
        business: &str,
        upload_key: i64,
        business_id: i64,
        referenced_file_ids: &[i64],
        current_user: &CurrentUser,
    ) -> Result<u64, AppError> {
        crate::file::service::FileService::sync_content_files(
            self,
            business,
            upload_key,
            business_id,
            referenced_file_ids,
            current_user,
        )
        .await
    }
    async fn soft_delete_business_files(
        &self,
        business: &str,
        business_id: i64,
        current_user: &CurrentUser,
    ) -> Result<u64, AppError> {
        crate::file::service::FileService::soft_delete_business_files(
            self,
            business,
            business_id,
            current_user,
        )
        .await
    }
    async fn get_file_url(&self, file_id: i64, is_inline: bool) -> Result<String, AppError> {
        crate::file::service::FileService::get_file_url(self, file_id, is_inline).await
    }
    async fn get_file_content(
        &self,
        file_id: i64,
        allow_private: bool,
    ) -> Result<FileContentDto, AppError> {
        crate::file::service::FileService::get_file_content(self, file_id, allow_private).await
    }
    async fn get_file_local_content(
        &self,
        file_id: i64,
        allow_private: bool,
    ) -> Result<FileLocalContentDto, AppError> {
        crate::file::service::FileService::get_file_local_content(self, file_id, allow_private)
            .await
    }
    async fn get_image_content(
        &self,
        file_id: i64,
        width: i32,
        height: i32,
    ) -> Result<ImageFileContentDto, AppError> {
        crate::file::service::FileService::get_image_content(self, file_id, width, height).await
    }
    async fn remote_upload_file(
        &self,
        dto: RemoteUploadFileDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        crate::file::service::FileService::remote_upload_file(self, dto, current_user).await
    }
    async fn delete_file(&self, id: i64, current_user: &CurrentUser) -> Result<bool, AppError> {
        crate::file::service::FileService::delete_file(self, id, current_user).await
    }
}
