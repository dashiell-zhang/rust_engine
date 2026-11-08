use serde::Deserialize;

/// 标识生成器配置
#[derive(Clone, Deserialize)]
pub struct IdentifierGeneratorSettings {
    /// 独立节点协调连接；宿主可在为空时明确选择业务 Redis 地址。
    #[serde(default)]
    pub redis_connection: String,

    /// 同一唯一性范围的所有进程必须共享此节点前缀。
    #[serde(default = "default_instance_name")]
    pub instance_name: String,

    /// 默认两天，允许 3 秒至 30 天；所有实例保持一致。
    #[serde(default = "default_lease_duration")]
    pub lease_duration_milliseconds: u64,
    /// 数据中心标识
    #[serde(default)]
    pub data_center_id: Option<u64>,

    /// 机器标识
    #[serde(default)]
    pub machine_id: Option<u64>,
}

fn default_instance_name() -> String {
    "NetEngine.Identifier".into()
}
fn default_lease_duration() -> u64 {
    172_800_000
}

impl Default for IdentifierGeneratorSettings {
    fn default() -> Self {
        Self {
            redis_connection: String::new(),
            instance_name: default_instance_name(),
            lease_duration_milliseconds: default_lease_duration(),
            data_center_id: None,
            machine_id: None,
        }
    }
}

impl IdentifierGeneratorSettings {
    /// 判断是否已显式配置节点标识
    pub fn has_fixed_sign(&self) -> bool {
        self.data_center_id.is_some() && self.machine_id.is_some()
    }

    pub(crate) fn validate(&self) -> Result<(), crate::IdentifierGeneratorError> {
        use crate::IdentifierGeneratorError as Error;
        if self.data_center_id.is_some() != self.machine_id.is_some() {
            return Err(Error::InvalidSettings(
                "固定节点必须同时指定数据中心与机器编号".into(),
            ));
        }
        if self.machine_id.is_some_and(|id| id > 31) {
            return Err(Error::InvalidMachineId);
        }
        if self.data_center_id.is_some_and(|id| id > 31) {
            return Err(Error::InvalidDataCenterId);
        }
        if !self.has_fixed_sign() {
            if self.instance_name.trim().is_empty() {
                return Err(Error::InvalidSettings("自动节点前缀不能为空".into()));
            }
            if !(3_000..=2_592_000_000).contains(&self.lease_duration_milliseconds) {
                return Err(Error::InvalidSettings(
                    "节点租约必须介于 3 秒与 30 天之间".into(),
                ));
            }
        }
        Ok(())
    }
}
