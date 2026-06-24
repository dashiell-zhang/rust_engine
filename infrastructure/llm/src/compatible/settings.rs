/// OpenAI 兼容协议的连接配置
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleSettings {
    /// 接口基址，通常为 https://host/v1
    pub base_url: String,

    /// Chat Completions 完整端点地址
    pub endpoint: Option<String>,

    /// 访问密钥
    pub api_key: String,
}

impl OpenAiCompatibleSettings {
    /// 创建配置
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            endpoint: None,
            api_key: api_key.into(),
        }
    }

    /// 使用完整 Chat Completions 端点创建配置
    pub fn new_endpoint(endpoint: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: String::new(),
            endpoint: Some(endpoint.into()),
            api_key: api_key.into(),
        }
    }
}
