use config::{Config, File};
use serde::Deserialize;

/// 数据库配置
#[derive(Deserialize, Clone)]
pub struct DatabaseSettings {
    #[serde(default)]
    pub modules: std::collections::BTreeMap<String, ModuleSettings>,
    /// 独立事件写库，不回退其他模型。
    #[serde(default)]
    pub event_bus_connection: String,
    /// 独立日志写库，不回退业务连接。
    #[serde(default)]
    pub log_connection: String,

    /// PostgreSQL 连接字符串
    #[serde(default)]
    pub connection: String,
}

/// 全局应用配置
#[derive(Deserialize, Clone)]
pub struct Settings {
    /// 数据库配置
    pub database: DatabaseSettings,
}

impl Settings {
    /// 固定读取工具可执行文件旁的配置，不继承 WebAPI 或进程环境覆盖。
    pub fn load(target: DatabaseTarget, module: Option<&str>) -> Result<Self, String> {
        let settings: Self = Config::builder()
            .add_source(File::from(default_config_path()?))
            .build()
            .map_err(|error| format!("配置文件加载失败: {error}"))?
            .try_deserialize()
            .map_err(|error| format!("配置文件解析失败: {error}"))?;
        let connection = settings.connection(target, module)?;
        if connection.trim().is_empty() {
            return Err(format!("所选数据库目标 {target:?} 缺少连接字符串"));
        }

        Ok(settings)
    }
}

/// 默认配置随工具构建复制，运行时按可执行文件定位，不依赖当前工作目录。
fn default_config_path() -> Result<std::path::PathBuf, String> {
    let executable = std::env::current_exe().map_err(|error| format!("定位工具失败：{error}"))?;
    Ok(executable
        .parent()
        .ok_or("工具可执行文件没有父目录")?
        .join("repository_tool.appsettings.toml"))
}

/// 明确选择写模型，同一个物理数据库也分别登记迁移历史。
#[derive(clap::ValueEnum, Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DatabaseTarget {
    Business,
    Log,
    #[value(alias = "eventbus")]
    #[serde(alias = "eventbus")]
    EventBus,
}
impl Settings {
    pub fn connection(&self, target: DatabaseTarget, module: Option<&str>) -> Result<&str, String> {
        if let Some(name) = module {
            let entry = self
                .database
                .modules
                .get(name)
                .ok_or_else(|| format!("工具未配置数据库模块: {name}"))?;
            if entry.model != target {
                return Err(format!("数据库模块 {name} 的模型与 --target 不一致"));
            }
            return Ok(&entry.connection);
        }
        Ok(match target {
            DatabaseTarget::Business => &self.database.connection,
            DatabaseTarget::Log => &self.database.log_connection,
            DatabaseTarget::EventBus => &self.database.event_bus_connection,
        })
    }
}

#[derive(Deserialize, Clone)]
pub struct ModuleSettings {
    pub model: DatabaseTarget,
    pub connection: String,
}
