/// DeepSeek Provider 默认接口地址
pub const DEFAULT_DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com/v1";

/// DeepSeek Provider 配置
#[derive(Debug, Clone)]
pub struct DeepSeekSettings {
    /// 接口基址
    pub base_url: String,

    /// 访问密钥
    pub api_key: String,
}

impl DeepSeekSettings {
    /// 使用指定接口地址创建配置
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }

    /// 使用默认接口地址创建配置
    pub fn with_default_base_url(api_key: impl Into<String>) -> Self {
        Self::new(DEFAULT_DEEPSEEK_BASE_URL, api_key)
    }
}
