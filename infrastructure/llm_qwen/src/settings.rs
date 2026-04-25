/// Qwen Provider 默认接口地址
pub const DEFAULT_QWEN_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";

/// Qwen Provider 配置
#[derive(Debug, Clone)]
pub struct QwenSettings {
    /// 接口基址
    pub base_url: String,

    /// 访问密钥
    pub api_key: String,
}

impl QwenSettings {
    /// 使用指定接口地址创建配置
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }

    /// 使用默认接口地址创建配置
    pub fn with_default_base_url(api_key: impl Into<String>) -> Self {
        Self::new(DEFAULT_QWEN_BASE_URL, api_key)
    }
}
