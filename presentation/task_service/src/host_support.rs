#![allow(clippy::type_complexity)]

use std::{future::Future, pin::Pin, sync::Arc};

use application_interface::sms::AppSmsSender;
use application_interface::{email::AppEmailSender, storage::AppFileStorage};
use distributed_cache::DistributedCache;
use distributed_cache_memory::InMemoryCache;
use distributed_cache_redis::RedisCache;
use distributed_lock::DistributedLock;
use distributed_lock_memory::InMemoryLock;
use distributed_lock_redis::RedisLock;
use email_smtp::{SmtpEmailSender, SmtpEmailSettings};
use file_storage_ali_cloud::{AliyunOssFileStorage, AliyunOssFileStorageSettings};
use file_storage_tencent_cloud::{TencentCosFileStorage, TencentCosFileStorageSettings};
use logger::{LoggerComponent, LoggerRuntime};
use logger_database::init_database_logger;
use logger_local_file::init_local_file_logger;
use sms_ali_cloud::{AliyunSmsSender, AliyunSmsSettings};
use sms_tencent_cloud::{TencentSmsSender, TencentSmsSettings};
use task_service_core::{
    TaskServiceLoggerBuilderContext,
    settings::{CacheComponent, FileStorageComponent, LockComponent, Settings, SmsComponent},
};

/// 构建任务宿主分布式锁
pub fn build_distributed_lock(
    settings: &Settings,
) -> Pin<Box<dyn Future<Output = Result<Arc<dyn DistributedLock>, String>> + Send + '_>> {
    Box::pin(async move {
        match settings.lock.component {
            LockComponent::InMemory => {
                Ok(Arc::new(InMemoryLock::new()) as Arc<dyn DistributedLock>)
            }
            LockComponent::Redis => Ok(Arc::new(
                RedisLock::new(&settings.lock.connection, &settings.lock.prefix)
                    .await
                    .map_err(|error| format!("Redis 分布式锁初始化失败: {error}"))?,
            ) as Arc<dyn DistributedLock>),
        }
    })
}

/// 构建任务宿主文件存储
pub fn build_file_storage(settings: &Settings) -> Result<Option<Arc<dyn AppFileStorage>>, String> {
    match settings.file.component {
        FileStorageComponent::Local => Ok(None),
        FileStorageComponent::AliyunOss => Ok(Some(Arc::new(
            AliyunOssFileStorage::new(AliyunOssFileStorageSettings {
                region: settings.file.aliyun_oss.region.clone(),
                use_internal_endpoint: settings.file.aliyun_oss.use_internal_endpoint,
                access_key_id: settings.file.aliyun_oss.access_key_id.clone(),
                access_key_secret: settings.file.aliyun_oss.access_key_secret.clone(),
                bucket_name: settings.file.aliyun_oss.bucket_name.clone(),
                url: settings.file.public_base_url.clone(),
            })
            .map_err(|error| error.message().to_string())?,
        ))),
        FileStorageComponent::TencentCos => Ok(Some(Arc::new(
            TencentCosFileStorage::new(TencentCosFileStorageSettings {
                app_id: settings.file.tencent_cos.app_id.clone(),
                region: settings.file.tencent_cos.region.clone(),
                secret_id: settings.file.tencent_cos.secret_id.clone(),
                secret_key: settings.file.tencent_cos.secret_key.clone(),
                bucket_name: settings.file.tencent_cos.bucket_name.clone(),
                url: settings.file.public_base_url.clone(),
            })
            .map_err(|error| error.message().to_string())?,
        ))),
    }
}

/// 构建任务宿主分布式缓存
pub fn build_distributed_cache(
    settings: &Settings,
) -> Pin<Box<dyn Future<Output = Result<Arc<dyn DistributedCache>, String>> + Send + '_>> {
    Box::pin(async move {
        match settings.cache.component {
            CacheComponent::InMemory => {
                Ok(Arc::new(InMemoryCache::new()) as Arc<dyn DistributedCache>)
            }
            CacheComponent::Redis => Ok(Arc::new(
                RedisCache::new(&settings.cache.connection, &settings.cache.prefix)
                    .await
                    .map_err(|error| format!("Redis 缓存初始化失败: {error}"))?,
            ) as Arc<dyn DistributedCache>),
        }
    })
}

/// 构建任务宿主日志运行时
pub fn build_logger_runtime(
    settings: &Settings,
    context: &TaskServiceLoggerBuilderContext,
) -> Result<LoggerRuntime, String> {
    match settings.logger.component {
        LoggerComponent::Database => init_database_logger(
            &settings.logger,
            Arc::clone(&context.db_pool),
            context.identifier_generator.clone(),
        ),
        LoggerComponent::LocalFile => init_local_file_logger(&settings.logger),
    }
    .map_err(|error| format!("日志组件初始化失败: {error}"))
}

/// 构建任务宿主短信发送器
pub fn build_sms_sender(settings: &Settings) -> Result<Arc<dyn AppSmsSender>, String> {
    match settings.sms.component {
        SmsComponent::Aliyun => Ok(Arc::new(AliyunSmsSender::new(AliyunSmsSettings {
            access_key_id: settings.sms.aliyun.access_key_id.clone(),
            access_key_secret: settings.sms.aliyun.access_key_secret.clone(),
        }))),
        SmsComponent::Tencent => Ok(Arc::new(TencentSmsSender::new(TencentSmsSettings {
            app_id: settings.sms.tencent.app_id.clone(),
            secret_id: settings.sms.tencent.secret_id.clone(),
            secret_key: settings.sms.tencent.secret_key.clone(),
        }))),
    }
}

/// 构建任务宿主邮件发送器
pub fn build_email_sender(settings: &Settings) -> Result<Arc<dyn AppEmailSender>, String> {
    Ok(Arc::new(SmtpEmailSender::new(SmtpEmailSettings {
        host: settings.message.smtp.host.clone(),
        port: settings.message.smtp.port,
        username: settings.message.smtp.username.clone(),
        password: settings.message.smtp.password.clone(),
    })))
}
