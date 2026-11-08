//! 三个入口复用的文件提供商选择；公共宿主和文件契约均不依赖此装配层。
use file_storage::AppFileStorage;
use file_storage_ali_cloud::{AliyunOssFileStorage, AliyunOssFileStorageSettings};
use file_storage_tencent_cloud::{TencentCosFileStorage, TencentCosFileStorageSettings};
use http_client::HttpClientFactory;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStorageComponent {
    #[default]
    Local,
    AliyunOss,
    TencentCos,
}

/// 只选择文件提供商；路径与公开策略仍由调用宿主负责。
#[derive(Clone, Default, Deserialize)]
pub struct FileStorageSettings {
    #[serde(default)]
    pub component: FileStorageComponent,
    #[serde(default)]
    pub aliyun_oss: AliyunOssFileStorageSettings,
    #[serde(default)]
    pub tencent_cos: TencentCosFileStorageSettings,
}

/// 从宿主已经合并的配置快照读取，沿用原有 [file] 节。
#[derive(Default, Deserialize)]
pub struct FileStorageConfiguration {
    #[serde(default)]
    pub file: FileStorageSettings,
}

impl FileStorageSettings {
    pub fn build(
        &self,
        public_base_url: &str,
        http_clients: Arc<HttpClientFactory>,
    ) -> Result<Option<Arc<dyn AppFileStorage>>, String> {
        match self.component {
            FileStorageComponent::Local => Ok(None),
            FileStorageComponent::AliyunOss => {
                let mut config = self.aliyun_oss.clone();
                config.url = public_base_url.to_owned();
                Ok(Some(Arc::new(
                    AliyunOssFileStorage::new(config, http_clients)
                        .map_err(|error| error.message().to_string())?,
                )))
            }
            FileStorageComponent::TencentCos => {
                let mut config = self.tencent_cos.clone();
                config.url = public_base_url.to_owned();
                Ok(Some(Arc::new(
                    TencentCosFileStorage::new(config, http_clients)
                        .map_err(|error| error.message().to_string())?,
                )))
            }
        }
    }
}
