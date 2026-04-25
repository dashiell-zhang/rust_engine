use config::{Config, Environment, File};
use serde::Deserialize;

/// 数据库配置
#[derive(Deserialize, Clone)]
pub struct DatabaseSettings {
    /// PostgreSQL 连接字符串
    pub connection: String,
}

/// 全局应用配置
#[derive(Deserialize, Clone)]
pub struct Settings {
    /// 数据库配置
    pub database: DatabaseSettings,
}

impl Settings {
    /// 加载数据库工具配置
    pub fn load(config_path: Option<String>, database_url: Option<String>) -> Result<Self, String> {
        let config_path = config_path
            .or_else(|| std::env::var("APP_CONFIG_PATH").ok())
            .unwrap_or_else(|| "presentation/admin_webapi/appsettings".to_string());

        let mut builder = Config::builder().add_source(File::with_name(&config_path));

        if let Some(environment_name) = resolve_config_environment() {
            let environment_config_path = format!("{config_path}.{environment_name}");
            builder = builder.add_source(File::with_name(&environment_config_path).required(false));
        }

        let mut settings: Settings = builder
            .add_source(Environment::with_prefix("APP").separator("__"))
            .build()
            .map_err(|error| format!("配置文件加载失败: {error}"))?
            .try_deserialize()
            .map_err(|error| format!("配置文件解析失败: {error}"))?;

        if let Some(database_url) = database_url {
            settings.database.connection = database_url;
        }

        if settings.database.connection.trim().is_empty() {
            return Err("数据库连接字符串不可以空".to_string());
        }

        Ok(settings)
    }
}

fn resolve_config_environment() -> Option<String> {
    std::env::var("APP_ENVIRONMENT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
