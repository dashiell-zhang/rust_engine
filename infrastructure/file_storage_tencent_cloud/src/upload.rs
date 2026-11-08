use std::collections::BTreeMap;

use common::app_error::AppError;
use futures_util::{StreamExt, TryStreamExt, stream};
use reqwest::{Method, header::ETAG};
use tokio::fs;

use super::{
    TencentCosFileStorage, cos_percent_encode,
    request::{RequestBody, file_digest},
    upload_xml::{self, CompletedUpload, InitiatedUpload, PartList, UploadList},
};

const DIVISION_SIZE: u64 = 5 * 1024 * 1024;
const SLICE_SIZE: u64 = 1024 * 1024;
// 5.4.51 的 MaxConcurrent=2，但进入等待的条件为 activeTasks > MaxConcurrent。
const CONCURRENT_PARTS: usize = 3;

struct UploadPart {
    offset: u64,
    length: u64,
    completed: bool,
    etag: Option<String>,
}

impl TencentCosFileStorage {
    pub(super) async fn upload_object(
        &self,
        local_path: &str,
        object_key: &str,
        public: bool,
        file_name: Option<&str>,
    ) -> Result<bool, AppError> {
        let length = fs::metadata(local_path)
            .await
            .map_err(|error| AppError::internal(format!("读取腾讯云 COS 上传文件失败: {error}")))?
            .len();
        let mut metadata = BTreeMap::from([(
            "x-cos-acl",
            if public { "public-read" } else { "private" }.to_owned(),
        )]);
        if let Some(name) = file_name {
            metadata.insert(
                "content-disposition",
                format!("attachment;filename*=UTF-8''{}", cos_percent_encode(name)),
            );
        }
        if length <= DIVISION_SIZE {
            let response = self
                .execute_request(
                    Method::PUT,
                    object_key,
                    &[],
                    metadata,
                    RequestBody::File {
                        path: local_path,
                        offset: 0,
                        length,
                    },
                )
                .await?;
            return Ok(response.status.is_success());
        }
        let count = length / SLICE_SIZE;
        if count >= 10_000 {
            return Err(AppError::internal(
                "腾讯云 COS 默认上传分片数达到上限，需要调整分片大小",
            ));
        }
        // SDK 使用向下取整的分片数，余数并入最后一片，而不是额外创建尾片。
        let mut parts: Vec<_> = (0..count)
            .map(|index| UploadPart {
                offset: index * SLICE_SIZE,
                length: if index + 1 == count {
                    length - index * SLICE_SIZE
                } else {
                    SLICE_SIZE
                },
                completed: false,
                etag: None,
            })
            .collect();
        let upload_id = match self
            .resume_upload(local_path, object_key, &mut parts)
            .await?
        {
            Some(id) => id,
            None => {
                let response = self
                    .execute_request(
                        Method::POST,
                        object_key,
                        &[("uploads", "")],
                        metadata,
                        RequestBody::Empty,
                    )
                    .await?;
                let result: InitiatedUpload =
                    upload_xml::parse(&response.body, "InitiateMultipartUploadResult")?;
                result
                    .id
                    .ok_or_else(|| AppError::internal("腾讯云 COS 初始化响应缺少上传编号"))?
            }
        };
        let pending: Vec<_> = parts
            .iter()
            .enumerate()
            .filter(|(_, part)| !part.completed)
            .map(|(index, part)| (index, part.offset, part.length))
            .collect();
        let mut uploads = stream::iter(pending)
            .map(|(index, offset, length)| {
                let upload_id = &upload_id;
                async move {
                    let number = (index + 1).to_string();
                    let response = self
                        .execute_request(
                            Method::PUT,
                            object_key,
                            &[("partNumber", &number), ("uploadId", upload_id)],
                            BTreeMap::new(),
                            RequestBody::File {
                                path: local_path,
                                offset,
                                length,
                            },
                        )
                        .await?;
                    let etag = response
                        .headers
                        .get(ETAG)
                        .map(|value| {
                            value
                                .to_str()
                                .map(str::to_owned)
                                .map_err(|_| AppError::internal("腾讯云 COS 分片 ETag 无效"))
                        })
                        .transpose()?;
                    Ok::<_, AppError>((index, etag))
                }
            })
            .buffer_unordered(CONCURRENT_PARTS);
        // 普通失败保留服务端 uploadId 和已完成分片，不自动 Abort 或删除云对象。
        while let Some((index, etag)) = uploads.try_next().await? {
            parts[index].completed = true;
            parts[index].etag = etag;
        }
        let body = upload_xml::completion(
            parts
                .iter()
                .enumerate()
                .map(|(index, part)| (index + 1, part.etag.as_deref())),
        );
        let response = self
            .execute_request(
                Method::POST,
                object_key,
                &[("uploadId", &upload_id)],
                BTreeMap::new(),
                RequestBody::Bytes(body),
            )
            .await?;
        let _: CompletedUpload =
            upload_xml::parse(&response.body, "CompleteMultipartUploadResult")?;
        Ok(response.status.is_success())
    }

    /// 默认续传检查 SDK 返回的首批未完成上传，从后往前选第一个同名对象。
    async fn resume_upload(
        &self,
        local_path: &str,
        object_key: &str,
        parts: &mut [UploadPart],
    ) -> Result<Option<String>, AppError> {
        let response = self
            .execute_request(
                Method::GET,
                "",
                &[("uploads", ""), ("prefix", object_key)],
                BTreeMap::new(),
                RequestBody::Empty,
            )
            .await?;
        let list: UploadList = upload_xml::parse(&response.body, "ListMultipartUploadsResult")?;
        let Some(upload) = list
            .uploads
            .into_iter()
            .rev()
            .find(|upload| upload.key.as_deref() == Some(object_key))
        else {
            // 前缀可能匹配别的对象。没有同名上传时创建新的上传，不等待缺失的回调。
            return Ok(None);
        };
        let id = upload
            .id
            .ok_or_else(|| AppError::internal("腾讯云 COS 未完成上传缺少编号"))?;
        let response = self
            .execute_request(
                Method::GET,
                object_key,
                &[("uploadId", &id)],
                BTreeMap::new(),
                RequestBody::Empty,
            )
            .await?;
        let remote: PartList = upload_xml::parse(&response.body, "ListPartsResult")?;
        let mut verified = Vec::new();
        let mut matches_all = true;
        for remote in remote.parts {
            let index = remote
                .number
                .as_deref()
                .and_then(|number| number.trim().parse::<usize>().ok())
                .and_then(|number| number.checked_sub(1))
                .filter(|index| *index < parts.len())
                .ok_or_else(|| AppError::internal("腾讯云 COS 已上传分片编号无效"))?;
            let part = &parts[index];
            let digest = file_digest(local_path, part.offset, part.length).await.ok();
            let matches = digest
                .zip(remote.etag.as_deref())
                .is_some_and(|(digest, etag)| {
                    let local = digest
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>();
                    local == etag.trim_matches('"')
                });
            matches_all &= matches;
            verified.push((index, remote.etag));
        }
        if !matches_all {
            return Ok(None);
        }
        for (index, etag) in verified {
            parts[index].completed = true;
            parts[index].etag = etag;
        }
        Ok(Some(id))
    }
}
