use common::img_helper;
use common::io_helper::{self, TemporaryFileGuard};
use repository::extensions::SoftDeleteQueryExt;
use repository_core::versioned::VersionedActiveModel;
use std::collections::HashSet;
use std::path::Path;

use application_interface::{
    context::{CurrentUser, FileContext},
    error::AppError,
};
use chrono::{TimeDelta, Utc};
use common::io_helper::file_length_to_string;
use repository::file;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait, IntoActiveModel,
    QueryFilter, QueryOrder,
};
use serde_json::json;
use tokio::fs;

use super::models::{
    FileContentDto, FileInfoDto, FileLocalContentDto, ImageFileContentDto, RemoteUploadFileDto,
    UploadFileDto,
};
use crate::write_support::{
    apply_soft_delete_audit_fields, build_create_audit_fields, write_data_update_log,
};

/// 文件应用服务
pub struct FileService<'a> {
    /// 文件模块上下文
    context: FileContext<'a>,
}

impl<'a> FileService<'a> {
    /// 创建文件应用服务
    pub fn new(context: FileContext<'a>) -> Self {
        Self { context }
    }

    /// 上传文件
    ///
    /// 先保存到本地目录，再按需上传到云存储并写入文件记录
    pub async fn upload_file(
        &self,
        dto: UploadFileDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        validate_upload_file(&dto)?;

        let mut dto = dto;
        if dto.key == Some(0) {
            dto.key = None;
        }

        let utc_now = Utc::now();
        let base_path = format!(
            "uploads/{}/{}/{}",
            utc_now.format("%Y"),
            utc_now.format("%m"),
            utc_now.format("%d")
        );
        let folder_path = Path::new(self.context.root_path).join(&base_path);

        fs::create_dir_all(&folder_path)
            .await
            .map_err(|error| AppError::internal(format!("创建文件目录失败: {error}")))?;

        let file_id = self
            .context
            .app
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成文件主键失败: {error}")))?;
        let extension = io_helper::file_extension(&dto.file_name).to_lowercase();
        let storage_file_name = format!("{file_id}{extension}");
        let file_path = folder_path.join(&storage_file_name);

        move_file(&dto.temp_file_path, &file_path).await?;

        let file_length = fs::metadata(&file_path)
            .await
            .map_err(|error| AppError::internal(format!("读取文件信息失败: {error}")))?
            .len() as i64;

        let mut is_success = true;

        if let Some(file_storage) = self.context.file_storage {
            is_success = file_storage
                .file_upload(
                    file_path
                        .to_str()
                        .ok_or_else(|| AppError::internal("本地文件路径非法"))?,
                    &base_path,
                    dto.is_public_read,
                    Some(&dto.file_name),
                )
                .await?;

            if is_success {
                io_helper::delete_file(&file_path);
            }
        }

        if !is_success {
            return Err(AppError::bad_request("文件上传失败"));
        }

        // 存储对象名与文件记录分别发号，保持独立身份。
        let file_id = self
            .context
            .app
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成文件记录主键失败: {error}")))?;
        let audit_fields = build_create_audit_fields(current_user);
        let normalized_sign = dto.sign.clone();
        let relative_path = format!("{base_path}/{storage_file_name}").replace('\\', "/");

        file::ActiveModel {
            row_version: Default::default(),
            id: Set(file_id),
            name: Set(dto.file_name.clone()),
            path: Set(relative_path.clone()),
            table: Set(dto.business.clone()),
            table_id: Set(dto.key),
            sign: Set(normalized_sign.clone()),
            sort: Set(0),
            create_time: Set(audit_fields.create_time),
            delete_time: Set(audit_fields.delete_time),
            create_user_id: Set(audit_fields.create_user_id),
            delete_user_id: Set(audit_fields.delete_user_id),
            length: Set(file_length),
            is_public_read: Set(dto.is_public_read),
        }
        .insert_versioned(self.context.app.db)
        .await
        .map_err(|error| AppError::internal(format!("写入文件记录失败: {error}")))?;

        write_data_update_log(
            self.context.app,
            "File",
            file_id,
            "create",
            None,
            Some(json!({
                "id": file_id,
                "name": dto.file_name.trim(),
                "path": relative_path,
                "table": dto.business.trim(),
                "table_id": dto.key,
                "sign": normalized_sign,
                "length": file_length,
                "is_public_read": dto.is_public_read,
            })),
            Some(current_user),
        )
        .await?;

        Ok(file_id.to_string())
    }

    /// 获取文件列表
    pub async fn get_file_list(
        &self,
        business: String,
        sign: Option<String>,
        key: Option<i64>,
        is_get_url: bool,
    ) -> Result<Vec<FileInfoDto>, AppError> {
        validate_business(&business)?;

        let mut db_query = file::Entity::find()
            .not_deleted(file::Column::DeleteTime)
            .filter(file::Column::Table.eq(business))
            .order_by_asc(file::Column::Sort)
            .order_by_asc(file::Column::Id);

        db_query = match key {
            Some(key) => db_query.filter(file::Column::TableId.eq(key)),
            None => db_query.filter(file::Column::TableId.is_null()),
        };

        if let Some(sign) = sign {
            db_query = db_query.filter(file::Column::Sign.eq(sign));
        }

        let models = db_query
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("查询文件列表失败: {error}")))?;

        let mut file_list = Vec::with_capacity(models.len());

        for model in models {
            let url = if is_get_url {
                Some(self.get_file_url(model.id, false).await?)
            } else {
                None
            };

            file_list.push(FileInfoDto {
                id: model.id.to_string(),
                name: model.name,
                length: model.length,
                length_text: file_length_to_string(model.length as u64),
                path: model.path,
                sign: model.sign,
                url,
            });
        }

        Ok(file_list)
    }

    /// 获取文件公开相对路径
    pub async fn get_file_relative_path(&self, file_id: i64) -> Result<String, AppError> {
        let model = self.get_file_model(file_id).await?;
        Ok(format!("/{}", model.path.trim_start_matches('/')))
    }

    /// 将上传批次文件绑定到正式业务记录
    pub async fn bind_files(
        &self,
        business: &str,
        sign: &str,
        upload_key: i64,
        business_id: i64,
        current_user: &CurrentUser,
    ) -> Result<u64, AppError> {
        self.bind_files_with_connection(
            self.context.app.db,
            business,
            sign,
            upload_key,
            business_id,
            current_user,
        )
        .await
    }

    /// 使用指定连接将上传批次文件绑定到正式业务记录
    pub(crate) async fn bind_files_with_connection<C>(
        &self,
        db: &C,
        business: &str,
        sign: &str,
        upload_key: i64,
        business_id: i64,
        current_user: &CurrentUser,
    ) -> Result<u64, AppError>
    where
        C: ConnectionTrait,
    {
        validate_business(business)?;

        if sign.trim().is_empty() {
            return Err(AppError::bad_request("sign 不可以空"));
        }

        if business_id <= 0 {
            return Err(AppError::bad_request("无效的 businessId"));
        }

        if upload_key == business_id {
            return Ok(0);
        }

        let models = file::Entity::find()
            .not_deleted(file::Column::DeleteTime)
            .filter(file::Column::Table.eq(business.trim()))
            .filter(file::Column::Sign.eq(sign.trim()))
            .filter(file::Column::TableId.eq(upload_key))
            .filter(file::Column::CreateUserId.eq(current_user.user_id))
            .order_by_asc(file::Column::Id)
            .all(db)
            .await
            .map_err(|error| AppError::internal(format!("查询待绑定文件失败: {error}")))?;

        let mut affected = 0;

        for model in models {
            let mut active = model.into_active_model();
            active.table_id = Set(Some(business_id));
            active
                .update_versioned(db)
                .await
                .map_err(|error| AppError::internal(format!("绑定文件失败: {error}")))?;
            affected += 1;
        }

        Ok(affected)
    }

    /// 根据正文实际引用同步文章正文文件
    pub async fn sync_content_files(
        &self,
        business: &str,
        upload_key: i64,
        business_id: i64,
        referenced_file_ids: &[i64],
        current_user: &CurrentUser,
    ) -> Result<u64, AppError> {
        self.sync_content_files_with_connection(
            self.context.app.db,
            business,
            upload_key,
            business_id,
            referenced_file_ids,
            current_user,
        )
        .await
    }

    /// 使用指定连接根据正文实际引用同步文章正文文件
    pub(crate) async fn sync_content_files_with_connection<C>(
        &self,
        db: &C,
        business: &str,
        upload_key: i64,
        business_id: i64,
        referenced_file_ids: &[i64],
        current_user: &CurrentUser,
    ) -> Result<u64, AppError>
    where
        C: ConnectionTrait,
    {
        validate_business(business)?;

        if business_id <= 0 {
            return Err(AppError::bad_request("无效的 businessId"));
        }

        let distinct_file_ids = referenced_file_ids.iter().copied().collect::<HashSet<_>>();

        if distinct_file_ids.iter().any(|file_id| *file_id <= 0) {
            return Err(AppError::bad_request("正文包含无效或不属于当前文章的文件"));
        }

        let models = file::Entity::find()
            .not_deleted(file::Column::DeleteTime)
            .filter(file::Column::Table.eq(business.trim()))
            .filter(file::Column::Sign.starts_with("content-"))
            .filter(
                Condition::any()
                    .add(file::Column::TableId.eq(business_id))
                    .add(
                        Condition::all()
                            .add(file::Column::TableId.eq(upload_key))
                            .add(file::Column::CreateUserId.eq(current_user.user_id)),
                    ),
            )
            .order_by_asc(file::Column::Id)
            .all(db)
            .await
            .map_err(|error| AppError::internal(format!("查询正文文件失败: {error}")))?;

        let available_file_ids = models.iter().map(|model| model.id).collect::<HashSet<_>>();

        if !distinct_file_ids.is_subset(&available_file_ids) {
            return Err(AppError::bad_request("正文包含无效或不属于当前文章的文件"));
        }

        for model in models {
            let file_id = model.id;
            let table_id = model.table_id;

            if distinct_file_ids.contains(&file_id) {
                if table_id == Some(upload_key) {
                    let mut active = model.into_active_model();
                    active.table_id = Set(Some(business_id));
                    active.update_versioned(db).await.map_err(|error| {
                        AppError::internal(format!("同步正文文件失败: {error}"))
                    })?;
                }
            } else {
                let mut active = model.into_active_model();
                apply_soft_delete_audit_fields(
                    &mut active.delete_time,
                    &mut active.delete_user_id,
                    current_user,
                );
                active
                    .update_versioned(db)
                    .await
                    .map_err(|error| AppError::internal(format!("同步正文文件失败: {error}")))?;
            }
        }

        Ok(distinct_file_ids.len() as u64)
    }

    /// 软删除正式业务记录关联的全部文件
    pub async fn soft_delete_business_files(
        &self,
        business: &str,
        business_id: i64,
        current_user: &CurrentUser,
    ) -> Result<u64, AppError> {
        self.soft_delete_business_files_with_connection(
            self.context.app.db,
            business,
            business_id,
            current_user,
        )
        .await
    }

    /// 使用指定连接软删除正式业务记录关联的全部文件
    pub(crate) async fn soft_delete_business_files_with_connection<C>(
        &self,
        db: &C,
        business: &str,
        business_id: i64,
        current_user: &CurrentUser,
    ) -> Result<u64, AppError>
    where
        C: ConnectionTrait,
    {
        validate_business(business)?;

        if business_id <= 0 {
            return Err(AppError::bad_request("无效的 businessId"));
        }

        let models = file::Entity::find()
            .not_deleted(file::Column::DeleteTime)
            .filter(file::Column::Table.eq(business.trim()))
            .filter(file::Column::TableId.eq(business_id))
            .order_by_asc(file::Column::Id)
            .all(db)
            .await
            .map_err(|error| AppError::internal(format!("查询待删除文件失败: {error}")))?;

        let affected = models.len() as u64;

        for model in models {
            let mut active = model.into_active_model();
            apply_soft_delete_audit_fields(
                &mut active.delete_time,
                &mut active.delete_user_id,
                current_user,
            );
            active
                .update_versioned(db)
                .await
                .map_err(|error| AppError::internal(format!("软删除业务文件失败: {error}")))?;
        }

        Ok(affected)
    }

    /// 获取文件访问地址
    pub async fn get_file_url(&self, file_id: i64, is_inline: bool) -> Result<String, AppError> {
        let model = self.get_file_model(file_id).await?;

        let file_storage = match self.context.file_storage {
            Some(storage) if !model.is_public_read => storage,
            _ => return Ok(format!("{}{}", self.context.public_base_url, model.path)),
        };
        file_storage
            .get_file_url(&model.path, TimeDelta::minutes(10), is_inline)?
            .ok_or_else(|| AppError::bad_request("文件临时授权地址获取失败"))
    }

    /// 获取文件内容
    pub async fn get_file_content(
        &self,
        file_id: i64,
        allow_private: bool,
    ) -> Result<FileContentDto, AppError> {
        let local_file = self.get_file_local_content(file_id, allow_private).await?;
        let bytes = fs::read(&local_file.local_path)
            .await
            .map_err(|error| AppError::internal(format!("读取文件失败: {error}")))?;

        Ok(FileContentDto {
            file_name: local_file.file_name,
            path: local_file.path,
            bytes,
        })
    }

    /// 获取文件本地读取信息
    ///
    /// 只返回本地路径，缺失文件由宿主响应层处理，不自动下载云文件。
    pub async fn get_file_local_content(
        &self,
        file_id: i64,
        allow_private: bool,
    ) -> Result<FileLocalContentDto, AppError> {
        let model = self.get_file_model(file_id).await?;

        if !model.is_public_read && !allow_private {
            return Err(AppError::unauthorized("当前文件不允许匿名访问"));
        }

        let local_path = Path::new(self.context.root_path).join(&model.path);

        Ok(FileLocalContentDto {
            file_name: model.name,
            path: model.path,
            local_path,
        })
    }

    /// 获取指定尺寸的图片内容
    pub async fn get_image_content(
        &self,
        file_id: i64,
        width: i32,
        height: i32,
    ) -> Result<ImageFileContentDto, AppError> {
        let file = self.get_file_local_content(file_id, true).await?;
        let resized_png = if width == 0 && height == 0 {
            None
        } else {
            let bytes = fs::read(&file.local_path)
                .await
                .map_err(|error| AppError::internal(format!("读取图片失败: {error}")))?;
            tokio::task::spawn_blocking(move || img_helper::resize_image(&bytes, width, height))
                .await
                .map_err(|error| AppError::internal(format!("图片处理任务失败: {error}")))?
                .map_err(|error| AppError::internal(format!("图片缩放失败: {error}")))?
        };
        Ok(ImageFileContentDto { file, resized_png })
    }

    /// 通过远程地址下载并上传文件
    pub async fn remote_upload_file(
        &self,
        dto: RemoteUploadFileDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        let temp_dir = Path::new(self.context.root_path).join("temps");
        fs::create_dir_all(&temp_dir)
            .await
            .map_err(|error| AppError::internal(format!("创建临时目录失败: {error}")))?;
        let temp_name = format!(
            "{}{}",
            uuid::Uuid::new_v4(),
            io_helper::file_extension(&dto.file_name)
        );
        let factory = self.context.http_client_factory;
        let path = common::http_client_helper::download_file(
            &factory.streaming_client(),
            &dto.url,
            &temp_dir,
            Some(&temp_name),
            factory.request_timeout(),
        )
        .await
        .map_err(|_| AppError::bad_request("远程文件下载失败"))?;
        let guard = TemporaryFileGuard::new(path);
        self.upload_file(
            UploadFileDto {
                business: dto.business,
                key: dto.key,
                sign: dto.sign,
                is_public_read: dto.is_public_read,
                file_name: dto.file_name,
                temp_file_path: guard.path().to_string_lossy().into_owned(),
            },
            current_user,
        )
        .await
    }

    /// 删除文件
    pub async fn delete_file(&self, id: i64, current_user: &CurrentUser) -> Result<bool, AppError> {
        let model = file::Entity::find_by_id(id)
            .not_deleted(file::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("查询文件失败: {error}")))?;

        let Some(model) = model else {
            return Ok(false);
        };

        let before_log = json!({
            "id": model.id,
            "name": model.name,
            "path": model.path,
            "table": model.table,
            "table_id": model.table_id,
            "sign": model.sign,
            "length": model.length,
            "is_public_read": model.is_public_read,
        });
        let mut active = model.clone().into_active_model();
        apply_soft_delete_audit_fields(
            &mut active.delete_time,
            &mut active.delete_user_id,
            current_user,
        );

        active
            .update_versioned(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("删除文件失败: {error}")))?;

        write_data_update_log(
            self.context.app,
            "File",
            id,
            "delete",
            Some(before_log),
            Some(json!({
                "id": id,
                "delete_user_id": current_user.user_id,
            })),
            Some(current_user),
        )
        .await?;

        Ok(true)
    }

    /// 获取文件记录
    async fn get_file_model(&self, file_id: i64) -> Result<file::Model, AppError> {
        let model = file::Entity::find_by_id(file_id)
            .not_deleted(file::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("查询文件失败: {error}")))?;

        model.ok_or_else(|| AppError::bad_request("通过指定的文件ID未找到任何文件"))
    }
}

/// 移动文件
async fn move_file(from: &str, to: &Path) -> Result<(), AppError> {
    match fs::rename(from, to).await {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(from, to)
                .await
                .map_err(|error| AppError::internal(format!("移动文件失败: {error}")))?;
            fs::remove_file(from)
                .await
                .map_err(|error| AppError::internal(format!("清理临时文件失败: {error}")))?;
            Ok(())
        }
    }
}

/// 校验上传文件请求
fn validate_upload_file(dto: &UploadFileDto) -> Result<(), AppError> {
    validate_business(&dto.business)?;

    if dto.temp_file_path.trim().is_empty() {
        return Err(AppError::bad_request("临时文件路径不可以空"));
    }

    Ok(())
}

/// 校验业务领域
fn validate_business(business: &str) -> Result<(), AppError> {
    if business.trim().is_empty() {
        return Err(AppError::bad_request("business 不可以空"));
    }

    Ok(())
}
