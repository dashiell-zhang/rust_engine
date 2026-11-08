//! 消息任务需要的配置由任务入口持有，公共调度核心无需知道提供商。
use email_smtp::SmtpEmailSettings;
use file_storage_support::{FileStorageConfiguration, FileStorageSettings};
use serde::Deserialize;
use sms_ali_cloud::AliyunSmsSettings;
use sms_tencent_cloud::TencentSmsSettings;
use task_service_core::settings::Settings;

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmsComponent {
    #[default]
    Aliyun,
    Tencent,
}

#[derive(Clone, Default, Deserialize)]
pub struct SmsSettings {
    #[serde(default)]
    pub component: SmsComponent,
    #[serde(default)]
    pub aliyun: AliyunSmsSettings,
    #[serde(default)]
    pub tencent: TencentSmsSettings,
}

#[derive(Clone, Default, Deserialize)]
pub struct MessageSettings {
    #[serde(default)]
    pub smtp: SmtpEmailSettings,
}

#[derive(Clone, Deserialize)]
pub struct FileSettings {
    #[serde(default = "default_file_root_path")]
    pub root_path: String,
    #[serde(default)]
    pub public_base_url: String,
}

impl Default for FileSettings {
    fn default() -> Self {
        Self {
            root_path: default_file_root_path(),
            public_base_url: String::new(),
        }
    }
}

#[derive(Clone, Default)]
pub struct TaskComponentSettings {
    pub file: FileSettings,
    pub file_storage: FileStorageSettings,
    pub sms: SmsSettings,
    pub message: MessageSettings,
}

#[derive(Deserialize)]
struct TaskComponentConfiguration {
    #[serde(default)]
    pub file: FileSettings,
    #[serde(default)]
    pub sms: SmsSettings,
    #[serde(default)]
    pub message: MessageSettings,
}

impl TaskComponentSettings {
    pub fn load(settings: &Settings) -> Result<Self, String> {
        let raw: TaskComponentConfiguration = settings.extension()?;
        // 提供商直接从同一快照解析，避免 flatten 缓冲丢失 config 的标量转换。
        let storage: FileStorageConfiguration = settings.extension()?;
        let mut config = Self {
            file: raw.file,
            file_storage: storage.file,
            sms: raw.sms,
            message: raw.message,
        };
        if config.file.root_path.trim().is_empty()
            || config.file.root_path == default_file_root_path()
        {
            config.file.root_path = "presentation/task_service/storage".to_string();
        }
        Ok(config)
    }
}

pub fn validate(settings: &Settings) -> Result<(), String> {
    TaskComponentSettings::load(settings).map(|_| ())
}

fn default_file_root_path() -> String {
    "storage".to_string()
}
