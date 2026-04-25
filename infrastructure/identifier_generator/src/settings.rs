use serde::Deserialize;

/// 标识生成器配置
#[derive(Clone, Default, Deserialize)]
pub struct IdentifierGeneratorSettings {
    /// 数据中心标识
    #[serde(default)]
    pub data_center_id: Option<u64>,

    /// 机器标识
    #[serde(default)]
    pub machine_id: Option<u64>,
}

impl IdentifierGeneratorSettings {
    /// 判断是否已显式配置节点标识
    pub fn has_fixed_sign(&self) -> bool {
        self.data_center_id.is_some() && self.machine_id.is_some()
    }
}
