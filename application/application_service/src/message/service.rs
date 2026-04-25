use application_interface::{
    context::MessageContext,
    email::{SendEmailAttachment, SendEmailRequest},
};
use application_model::message::models::{SendEmailDto, SendEmailMessage};
use chrono::Utc;
use common::app_error::AppError;
use common::io_helper;
use repository::extensions::SoftDeleteQueryExt;
use repository::{file, queue_task, queue_task::QueueTaskStatus};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QuerySelect,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

struct PreparedSendEmailRequest {
    request: SendEmailRequest,
    temp_files: Vec<PathBuf>,
}

struct ResolvedAttachmentFile {
    name: String,
    local_path: PathBuf,
}

/// 消息应用服务，负责邮件发送
pub struct MessageService<'a> {
    /// 消息模块上下文
    context: MessageContext<'a>,
}

impl<'a> MessageService<'a> {
    /// 创建消息应用服务
    pub fn new(context: MessageContext<'a>) -> Self {
        Self { context }
    }

    /// 发送邮件
    pub async fn send_email(&self, dto: SendEmailDto) -> Result<(), AppError> {
        let prepared = self.build_send_email_request(dto).await?;
        let result = self.send_request(prepared.request).await;
        self.cleanup_temp_files(&prepared.temp_files);
        result
    }

    /// 投递邮件发送队列任务
    pub async fn send_email_to_queue(&self, dto: SendEmailDto) -> Result<(), AppError> {
        let queue_task_name = self
            .context
            .email_queue_task_name
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .ok_or_else(|| AppError::internal("邮件队列任务名称未配置"))?;

        let recipient_count = dto.to_addresses.len();
        let attachment_count = dto.file_id_list.as_ref().map_or(0, Vec::len);
        let parameter = serde_json::to_string(&dto)
            .map_err(|error| AppError::internal(format!("邮件发送任务参数序列化失败: {error}")))?;
        let queue_task_id = self
            .context
            .app
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成邮件队列任务主键失败: {error}")))?;

        queue_task::ActiveModel {
            id: Set(queue_task_id),
            name: Set(queue_task_name.to_string()),
            parameter: Set(Some(parameter)),
            status: Set(QueueTaskStatus::Pending),
            count: Set(0),
            first_time: Set(None),
            last_time: Set(None),
            success_time: Set(None),
            create_time: Set(Utc::now().into()),
            delete_time: Set(None),
            plan_time: Set(None),
            worker_id: Set(None),
            lease_expire_time: Set(None),
            callback_name: Set(None),
            callback_parameter: Set(None),
            child_success_time: Set(None),
            parent_task_id: Set(None),
        }
        .insert(self.context.app.db)
        .await
        .map_err(|error| AppError::internal(format!("投递邮件发送队列任务失败: {error}")))?;

        tracing::info!(
            target: "message.email.queue",
            queue_task_id,
            recipient_count,
            attachment_count,
            "邮件发送任务已入队"
        );

        Ok(())
    }

    /// 根据队列参数发送邮件消息
    pub async fn send_queue_message(&self, parameter: &str) -> Result<(), AppError> {
        match serde_json::from_str::<SendEmailMessage>(parameter) {
            Ok(message) => self.send_message(message).await,
            Err(old_format_error) => {
                let dto = serde_json::from_str::<SendEmailDto>(parameter).map_err(|new_format_error| {
                    AppError::bad_request(format!(
                        "邮件发送任务参数格式无效: 旧格式错误: {old_format_error}; 新格式错误: {new_format_error}"
                    ))
                })?;
                let prepared = self.build_send_email_request(dto).await?;
                let result = self.send_request(prepared.request).await;
                self.cleanup_temp_files(&prepared.temp_files);
                result
            }
        }
    }

    /// 构建直接发送请求
    async fn build_send_email_request(
        &self,
        dto: SendEmailDto,
    ) -> Result<PreparedSendEmailRequest, AppError> {
        let (attachment_files, temp_files) = self
            .resolve_attachment_files(dto.file_id_list.as_ref())
            .await?;

        Ok(PreparedSendEmailRequest {
            request: SendEmailRequest {
                to_addresses: dto.to_addresses,
                subject: dto.subject,
                body: dto.body,
                is_body_html: dto.is_body_html,
                from_display_name: dto.from_display_name,
                attachments: attachment_files
                    .into_iter()
                    .map(|attachment| SendEmailAttachment::LocalFile {
                        name: attachment.name,
                        path: attachment.local_path.to_string_lossy().into_owned(),
                    })
                    .collect(),
            },
            temp_files,
        })
    }

    /// 解析附件文件并确保本地可访问
    async fn resolve_attachment_files(
        &self,
        file_ids: Option<&Vec<i64>>,
    ) -> Result<(Vec<ResolvedAttachmentFile>, Vec<PathBuf>), AppError> {
        let Some(file_ids) = file_ids.filter(|item| !item.is_empty()) else {
            return Ok((Vec::new(), Vec::new()));
        };

        let records = file::Entity::find()
            .not_deleted(file::Column::DeleteTime)
            .filter(file::Column::Id.is_in(file_ids.clone()))
            .select_only()
            .column(file::Column::Id)
            .column(file::Column::Name)
            .column(file::Column::Path)
            .into_tuple::<(i64, String, String)>()
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("查询附件记录失败: {error:?}")))?;

        let record_map = records
            .into_iter()
            .map(|(id, name, path)| (id, (name, path)))
            .collect::<HashMap<_, _>>();
        let mut attachments = Vec::with_capacity(file_ids.len());
        let mut temp_files = Vec::new();

        for file_id in file_ids {
            let (name, path) = record_map
                .get(file_id)
                .ok_or_else(|| AppError::bad_request(format!("附件不存在或已删除 [{file_id}]")))?;
            let id = *file_id;
            let local_path = self.resolve_existing_local_path(path).await;
            let local_path =
                local_path.unwrap_or_else(|| Path::new(self.context.root_path).join(path));
            let mut downloaded_temp = false;

            if fs::metadata(&local_path).await.is_err() {
                let storage = self.context.file_storage.ok_or_else(|| {
                    AppError::internal(format!("附件文件不存在且未配置文件存储 [{id}]"))
                })?;

                if let Some(parent) = local_path.parent() {
                    fs::create_dir_all(parent).await.map_err(|error| {
                        AppError::internal(format!("创建附件目录失败: {error}"))
                    })?;
                }

                let download_success = storage
                    .file_download(
                        path,
                        local_path
                            .to_str()
                            .ok_or_else(|| AppError::internal("附件本地路径非法"))?,
                    )
                    .await
                    .map_err(|error| {
                        AppError::internal(format!("读取附件失败 [{id}]: {error:?}"))
                    })?;

                if !download_success {
                    return Err(AppError::internal(format!("读取附件失败 [{id}]")));
                }

                downloaded_temp = true;
            }

            fs::metadata(&local_path)
                .await
                .map_err(|error| AppError::internal(format!("读取附件失败 [{id}]: {error:?}")))?;

            if downloaded_temp {
                temp_files.push(local_path.clone());
            }

            attachments.push(ResolvedAttachmentFile {
                name: name.to_owned(),
                local_path,
            });
        }

        Ok((attachments, temp_files))
    }

    /// 解析已存在的本地附件路径
    async fn resolve_existing_local_path(&self, relative_path: &str) -> Option<PathBuf> {
        for root_path in self.attachment_root_candidates() {
            let local_path = Path::new(root_path).join(relative_path);
            if fs::metadata(&local_path).await.is_ok() {
                return Some(local_path);
            }
        }

        None
    }

    /// 枚举附件根目录候选
    fn attachment_root_candidates(&self) -> Vec<&str> {
        let mut candidates = Vec::new();

        let primary_root = self.context.root_path.trim();
        if !primary_root.is_empty() {
            candidates.push(primary_root);
        }

        if let Some(root_paths) = self.context.attachment_root_paths {
            for root_path in root_paths {
                let root_path = root_path.trim();
                if root_path.is_empty() {
                    continue;
                }

                if candidates.contains(&root_path) {
                    continue;
                }

                candidates.push(root_path);
            }
        }

        candidates
    }

    /// 发送已准备好的邮件消息
    async fn send_message(&self, message: SendEmailMessage) -> Result<(), AppError> {
        self.send_request(SendEmailRequest {
            to_addresses: message.to_addresses,
            subject: message.subject,
            body: message.body,
            is_body_html: message.is_body_html,
            from_display_name: message.from_display_name,
            attachments: message
                .attachments
                .into_iter()
                .map(|item| SendEmailAttachment::Memory {
                    name: item.name,
                    content: item.content,
                })
                .collect(),
        })
        .await
    }

    /// 发送邮件请求
    async fn send_request(&self, request: SendEmailRequest) -> Result<(), AppError> {
        let recipient_count = request.to_addresses.len();
        let attachment_count = request.attachments.len();
        let total_attachment_bytes = self.total_attachment_bytes(&request).await;

        self.context
            .email_sender
            .ok_or_else(|| AppError::internal("邮件发送器未配置"))?
            .send_email(request)
            .await?;

        tracing::info!(
            target: "message.email.send",
            recipient_count,
            attachment_count,
            total_attachment_bytes,
            "邮件发送成功"
        );

        Ok(())
    }

    /// 估算附件总字节数
    async fn total_attachment_bytes(&self, request: &SendEmailRequest) -> usize {
        let mut total = 0usize;

        for attachment in &request.attachments {
            match attachment {
                SendEmailAttachment::LocalFile { path, .. } => {
                    if let Ok(metadata) = fs::metadata(path).await
                        && let Ok(length) = usize::try_from(metadata.len())
                    {
                        total = total.saturating_add(length);
                    }
                }
                SendEmailAttachment::Memory { content, .. } => {
                    total = total.saturating_add(content.len());
                }
            }
        }

        total
    }

    /// 清理临时附件文件
    fn cleanup_temp_files(&self, temp_files: &[PathBuf]) {
        for temp_file in temp_files {
            io_helper::delete_file(temp_file);
        }
    }
}
