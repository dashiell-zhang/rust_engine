use common::img_helper;
use common::io_helper;
use repository::extensions::SoftDeleteQueryExt;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use application_interface::{
    context::{CurrentUser, FileContext},
    error::AppError,
};
use chrono::Utc;
use common::io_helper::file_length_to_string;
use repository::file;
use reqwest::{Url, redirect::Policy};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder,
};
use serde_json::json;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::models::{
    FileContentDto, FileInfoDto, FileLocalContentDto, RemoteUploadFileDto, UploadFileDto,
};
use crate::write_support::{
    apply_soft_delete_audit_fields, build_create_audit_fields, normalize_optional_text,
    write_data_update_log,
};

/// 远程上传文件大小上限
const REMOTE_UPLOAD_MAX_BYTES: usize = 10 * 1024 * 1024;

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
        let extension = Path::new(dto.file_name.trim())
            .extension()
            .and_then(|item| item.to_str())
            .map(|item| item.to_ascii_lowercase())
            .filter(|item| !item.is_empty());
        let storage_file_name = match extension {
            Some(extension) => format!("{file_id}.{extension}"),
            None => file_id.to_string(),
        };
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
                    Some(dto.file_name.trim()),
                )
                .await?;

            if is_success {
                io_helper::delete_file(&file_path);
            }
        }

        if !is_success {
            return Err(AppError::internal("文件上传失败"));
        }

        let audit_fields = build_create_audit_fields(current_user);
        let normalized_sign = normalize_optional_text(Some(dto.sign.clone())).unwrap_or_default();
        let relative_path = format!("{base_path}/{storage_file_name}").replace('\\', "/");

        file::ActiveModel {
            id: Set(file_id),
            name: Set(dto.file_name.trim().to_string()),
            path: Set(relative_path.clone()),
            table: Set(dto.business.trim().to_string()),
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
        .insert(self.context.app.db)
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
            .filter(file::Column::Table.eq(business.trim()))
            .order_by_asc(file::Column::Sort)
            .order_by_asc(file::Column::Id);

        db_query = match key {
            Some(key) => db_query.filter(file::Column::TableId.eq(key)),
            None => db_query.filter(file::Column::TableId.is_null()),
        };

        if let Some(sign) = sign
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
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
                is_public_read: model.is_public_read,
            });
        }

        Ok(file_list)
    }

    /// 获取文件公开相对路径
    pub async fn get_file_relative_path(&self, file_id: i64) -> Result<String, AppError> {
        let model = self.get_file_model(file_id).await?;
        Ok(format!("/{}", model.path.trim_start_matches('/')))
    }

    /// 批量更新文件关联业务主键
    pub async fn rebind_file_key(
        &self,
        business: String,
        sign: Option<String>,
        from_key: i64,
        to_key: i64,
    ) -> Result<u64, AppError> {
        validate_business(&business)?;

        if from_key <= 0 {
            return Err(AppError::bad_request("无效的 fromKey"));
        }

        if to_key <= 0 {
            return Err(AppError::bad_request("无效的 toKey"));
        }

        if from_key == to_key {
            return Ok(0);
        }

        let mut db_query = file::Entity::find()
            .not_deleted(file::Column::DeleteTime)
            .filter(file::Column::Table.eq(business.trim()))
            .filter(file::Column::TableId.eq(from_key))
            .order_by_asc(file::Column::Id);

        if let Some(sign) = sign
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            db_query = db_query.filter(file::Column::Sign.eq(sign));
        }

        let models = db_query
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("查询待改绑文件失败: {error}")))?;

        let mut affected = 0;

        for model in models {
            let mut active = model.into_active_model();
            active.table_id = Set(Some(to_key));
            active
                .update(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("更新文件关联主键失败: {error}")))?;
            affected += 1;
        }

        Ok(affected)
    }

    /// 获取文件访问地址
    pub async fn get_file_url(&self, file_id: i64, is_inline: bool) -> Result<String, AppError> {
        let model = self.get_file_model(file_id).await?;

        if model.is_public_read || self.context.file_storage.is_none() {
            return Ok(format!(
                "{}{}",
                self.context.public_base_url.trim_end_matches('/'),
                if self.context.public_base_url.trim().is_empty() {
                    model.path.clone()
                } else {
                    format!("/{}", model.path.trim_start_matches('/'))
                }
            ));
        }

        let temp_url = self
            .context
            .file_storage
            .and_then(|file_storage| {
                file_storage.get_file_url(&model.path, Duration::from_secs(600), is_inline)
            })
            .ok_or_else(|| AppError::internal("文件临时授权地址获取失败"))?;

        Ok(temp_url)
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
    /// 本地文件存在时直接返回物理路径；云存储文件首次读取时下载到本地缓存，后续可按物理文件响应。
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

        if fs::metadata(&local_path).await.is_err()
            && let Some(file_storage) = self.context.file_storage
        {
            if let Some(parent) = local_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|error| AppError::internal(format!("创建文件目录失败: {error}")))?;
            }

            let download_success = file_storage
                .file_download(
                    &model.path,
                    local_path
                        .to_str()
                        .ok_or_else(|| AppError::internal("本地文件路径非法"))?,
                )
                .await?;

            if !download_success {
                return Err(AppError::bad_request("文件不存在"));
            }
        }

        if fs::metadata(&local_path).await.is_err() {
            return Err(AppError::bad_request("文件不存在"));
        }

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
        width: u32,
        height: u32,
    ) -> Result<FileContentDto, AppError> {
        let mut file = self.get_file_content(file_id, true).await?;
        file.bytes = img_helper::resize_image(&file.bytes, width, height)
            .map_err(|error| AppError::bad_request(format!("图片缩放失败: {error}")))?;
        Ok(file)
    }

    /// 通过远程地址下载并上传文件
    pub async fn remote_upload_file(
        &self,
        dto: RemoteUploadFileDto,
        current_user: &CurrentUser,
    ) -> Result<String, AppError> {
        validate_remote_upload_file(&dto)?;

        let url = parse_remote_upload_url(&dto.url)?;
        let target = validate_remote_upload_target(&url).await?;

        let client = build_remote_upload_client(self.context.http_client_factory, &target)
            .map_err(|error| AppError::internal(format!("创建远程下载客户端失败: {error}")))?;

        let mut response = common::http_client_helper::get(&client, url.as_str(), None)
            .await
            .map_err(|error| AppError::bad_request(format!("远程文件下载失败: {error}")))?;

        if !response.status().is_success() {
            return Err(AppError::bad_request(format!(
                "远程文件下载失败，响应状态码: {}",
                response.status()
            )));
        }

        if let Some(content_length) = response.content_length()
            && content_length > REMOTE_UPLOAD_MAX_BYTES as u64
        {
            return Err(AppError::bad_request(format!(
                "远程文件大小超过限制，最大允许 {} 字节",
                REMOTE_UPLOAD_MAX_BYTES
            )));
        }

        let temp_dir_path = Path::new(self.context.root_path).join("temps");
        fs::create_dir_all(&temp_dir_path)
            .await
            .map_err(|error| AppError::internal(format!("创建临时目录失败: {error}")))?;

        let temp_file_name = format!(
            "{}{}",
            uuid::Uuid::new_v4(),
            Path::new(&dto.file_name)
                .extension()
                .and_then(|item| item.to_str())
                .map(|item| format!(".{item}"))
                .unwrap_or_default()
        );
        let temp_file_path = temp_dir_path.join(temp_file_name);

        let mut temp_file = fs::File::create(&temp_file_path)
            .await
            .map_err(|error| AppError::internal(format!("创建临时文件失败: {error}")))?;
        let mut total_size = 0_usize;

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| AppError::bad_request(format!("读取远程文件内容失败: {error}")))?
        {
            total_size += chunk.len();
            if total_size > REMOTE_UPLOAD_MAX_BYTES {
                let _ = fs::remove_file(&temp_file_path).await;
                return Err(AppError::bad_request(format!(
                    "远程文件大小超过限制，最大允许 {} 字节",
                    REMOTE_UPLOAD_MAX_BYTES
                )));
            }

            temp_file
                .write_all(&chunk)
                .await
                .map_err(|error| AppError::internal(format!("写入临时文件失败: {error}")))?;
        }

        if total_size == 0 {
            let _ = fs::remove_file(&temp_file_path).await;
            return Err(AppError::bad_request("远程文件内容为空"));
        }

        temp_file
            .flush()
            .await
            .map_err(|error| AppError::internal(format!("刷新临时文件失败: {error}")))?;

        let result = self
            .upload_file(
                UploadFileDto {
                    business: dto.business,
                    key: dto.key,
                    sign: dto.sign,
                    is_public_read: dto.is_public_read,
                    file_name: dto.file_name,
                    temp_file_path: temp_file_path.to_string_lossy().to_string(),
                },
                current_user,
            )
            .await;

        let _ = fs::remove_file(&temp_file_path).await;

        result
    }

    /// 删除文件
    pub async fn delete_file(&self, id: i64, current_user: &CurrentUser) -> Result<bool, AppError> {
        validate_file_id(id)?;

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
            .update(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("删除文件失败: {error}")))?;

        if let Some(file_storage) = self.context.file_storage {
            let _ = file_storage.file_delete(&model.path).await;
        } else {
            let _ = io_helper::delete_file(Path::new(self.context.root_path).join(&model.path));
        }

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
        validate_file_id(file_id)?;

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

    if dto.file_name.trim().is_empty() {
        return Err(AppError::bad_request("文件名不可以空"));
    }

    if dto.temp_file_path.trim().is_empty() {
        return Err(AppError::bad_request("临时文件路径不可以空"));
    }

    Ok(())
}

/// 校验远程上传文件请求
fn validate_remote_upload_file(dto: &RemoteUploadFileDto) -> Result<(), AppError> {
    validate_business(&dto.business)?;

    if dto.url.trim().is_empty() {
        return Err(AppError::bad_request("url 不可以为空"));
    }

    if dto.file_name.trim().is_empty() {
        return Err(AppError::bad_request("file_name 不可以为空"));
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

/// 校验文件 ID
fn validate_file_id(file_id: i64) -> Result<(), AppError> {
    if file_id <= 0 {
        return Err(AppError::bad_request("无效的 fileId"));
    }

    Ok(())
}

/// 解析远程上传地址
fn parse_remote_upload_url(value: &str) -> Result<Url, AppError> {
    let url = Url::parse(value.trim())
        .map_err(|error| AppError::bad_request(format!("远程文件地址格式无效: {error}")))?;

    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err(AppError::bad_request(
            "远程文件地址仅支持 http 或 https 协议",
        )),
    }
}

/// 远程上传请求目标
struct RemoteUploadTarget {
    /// 原始主机名
    host: String,

    /// 已校验的解析地址
    resolved_address: Option<SocketAddr>,
}

/// 校验远程上传地址是否命中高风险主机
async fn validate_remote_upload_target(url: &Url) -> Result<RemoteUploadTarget, AppError> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::bad_request("远程文件地址缺少主机名"))?
        .to_string();

    if host.eq_ignore_ascii_case("localhost") {
        return Err(AppError::bad_request("不允许访问本地主机地址"));
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(AppError::bad_request(
                "远程文件地址不允许指向本地或内网地址",
            ));
        }

        return Ok(RemoteUploadTarget {
            host,
            resolved_address: None,
        });
    }

    let port = url.port_or_known_default().unwrap_or(80);
    let lookup_host = host.clone();

    let resolved: Vec<SocketAddr> = tokio::task::spawn_blocking(move || {
        use std::net::ToSocketAddrs;
        (lookup_host.as_str(), port)
            .to_socket_addrs()
            .map(|iter| iter.collect::<Vec<_>>())
    })
    .await
    .map_err(|_| AppError::bad_request("DNS 解析任务异常"))?
    .map_err(|error| AppError::bad_request(format!("解析远程文件地址失败: {error}")))?;

    if resolved.is_empty() {
        return Err(AppError::bad_request("远程文件地址未解析到可用地址"));
    }

    for address in &resolved {
        if is_private_ip(address.ip()) {
            return Err(AppError::bad_request(
                "远程文件地址不允许指向本地或内网地址",
            ));
        }
    }

    Ok(RemoteUploadTarget {
        host,
        resolved_address: resolved.first().copied(),
    })
}

/// 构建远程上传下载客户端
fn build_remote_upload_client(
    http_client_factory: &application_interface::http_client::HttpClientFactory,
    target: &RemoteUploadTarget,
) -> Result<reqwest::Client, reqwest::Error> {
    http_client_factory.build_client_with(|builder| {
        let builder = builder.redirect(Policy::none());

        match target.resolved_address {
            Some(address) => builder.resolve(&target.host, address),
            None => builder,
        }
    })
}

/// 判断 IP 是否属于本地或内网范围
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_private()
                || ipv4.is_loopback()
                || ipv4.is_link_local()
                || ipv4.is_broadcast()
                || ipv4.is_documentation()
                || ipv4.is_unspecified()
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()
                || ipv6.is_multicast()
                || ipv6.is_unspecified()
                || ipv6.is_unique_local()
                || ipv6.is_unicast_link_local()
        }
    }
}
