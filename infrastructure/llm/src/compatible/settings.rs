/// OpenAI 兼容 Provider 的连接配置
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleSettings {
    /// 接口基址，通常为 https://host/v1
    pub base_url: String,

    /// 访问密钥
    pub api_key: String,
}

impl OpenAiCompatibleSettings {
    /// 创建配置
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }
}
