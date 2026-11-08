use application_interface::{
    context::MessageContext,
    email::{SendEmailAttachment, SendEmailRequest},
};
use application_model::message::models::SendEmailDto;
use common::{app_error::AppError, io_helper};
use repository::{extensions::SoftDeleteEntityExt, file};
use sea_orm::{ColumnTrait, QueryFilter, QuerySelect};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};
use tokio::fs;

/// 邮件用例；附件按记录路径相对于宿主内容目录解析。
pub struct MessageService<'a> {
    context: MessageContext<'a>,
}

impl<'a> MessageService<'a> {
    pub fn new(context: MessageContext<'a>) -> Self {
        Self { context }
    }

    /// 发送成功后清理本次从存储提供商下载的文件；失败保留文件。
    pub async fn send_email(&self, dto: SendEmailDto) -> Result<(), AppError> {
        let (attachments, temp_files) = self
            .resolve_attachment_files(dto.file_id_list.as_deref())
            .await?;
        let to_addresses = dto
            .to_addresses
            .ok_or_else(|| AppError::internal("邮件收件人列表为空"))?
            .into_iter()
            .map(|address| address.ok_or_else(|| AppError::internal("邮件收件人地址为空")))
            .collect::<Result<Vec<_>, _>>()?;
        self.context
            .email_sender
            .ok_or_else(|| AppError::internal("邮件发送器未配置"))?
            .send_email(SendEmailRequest {
                to_addresses,
                subject: dto.subject.unwrap_or_default(),
                body: dto.body.unwrap_or_default(),
                is_body_html: dto.is_body_html,
                from_display_name: dto.from_display_name,
                attachments,
            })
            .await?;
        for path in temp_files {
            io_helper::delete_file(path);
        }
        Ok(())
    }

    async fn resolve_attachment_files(
        &self,
        file_ids: Option<&[i64]>,
    ) -> Result<(Vec<SendEmailAttachment>, Vec<PathBuf>), AppError> {
        let Some(file_ids) = file_ids else {
            return Ok((Vec::new(), Vec::new()));
        };
        let records = file::Entity::find_active()
            .filter(file::Column::Id.is_in(file_ids.iter().copied()))
            .select_only()
            .column(file::Column::Id)
            .column(file::Column::Name)
            .column(file::Column::Path)
            .into_tuple::<(i64, String, String)>()
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("查询附件记录失败: {error}")))?;
        let records = records
            .into_iter()
            .map(|(id, name, path)| (id, (name, path)))
            .collect::<HashMap<_, _>>();
        let mut attachments = Vec::with_capacity(file_ids.len());
        let mut paths = HashSet::new();
        let mut temp_files = Vec::new();
        for file_id in file_ids {
            let (name, path) = records
                .get(file_id)
                .ok_or_else(|| AppError::internal(format!("附件记录不存在或已删除 [{file_id}]")))?;
            let local_path = self.context.root_path.join(path);
            if let Some(parent) = local_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .map_err(|error| AppError::internal(format!("创建附件目录失败: {error}")))?;
            }
            if let Some(storage) = self.context.file_storage {
                let target = local_path
                    .to_str()
                    .ok_or_else(|| AppError::internal("附件本地路径非法"))?;
                if !storage.file_download(path, target).await? {
                    tracing::error!(target: "message.email.attachment", file_id, "文件下载失败");
                    return Err(AppError::internal(format!("{file_id}文件下载失败")));
                }
                temp_files.push(local_path.clone());
            }
            // 源 Dictionary.Add 拒绝重复路径，不折叠或重复发送同一个附件。
            let local_path_text = local_path
                .to_str()
                .ok_or_else(|| AppError::internal("附件本地路径非法"))?
                .to_owned();
            if !paths.insert(local_path_text.clone()) {
                return Err(AppError::internal("邮件附件路径重复"));
            }
            attachments.push(SendEmailAttachment::LocalFile {
                name: name.clone(),
                path: local_path_text,
            });
        }
        Ok((attachments, temp_files))
    }
}
