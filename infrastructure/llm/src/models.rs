use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 对话消息角色
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    /// 系统消息，用于设定行为和规则
    System,

    /// 用户消息
    User,

    /// 模型输出消息
    Assistant,

    /// 工具调用消息
    Tool,
}

/// 对话消息
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// 消息角色
    pub role: ChatRole,

    /// 消息内容
    pub content: String,
}

impl ChatMessage {
    /// 创建系统消息
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }

    /// 创建用户消息
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }

    /// 创建助手消息
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }
}

/// 对话请求
#[derive(Debug, Clone)]
pub struct ChatRequest {
    /// 模型名称
    pub model: String,

    /// 消息列表，按时间顺序排列
    pub messages: Vec<ChatMessage>,

    /// 随机性控制（通常 0~2）
    pub temperature: Option<f64>,

    /// 最大生成 token 数
    pub max_tokens: Option<i32>,

    /// 额外请求参数
    pub extra_body: Option<Map<String, Value>>,
}

impl ChatRequest {
    /// 创建对话请求
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            max_tokens: None,
            extra_body: None,
        }
    }

    /// 设置 temperature
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// 设置最大 token 数
    pub fn with_max_tokens(mut self, max_tokens: i32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// 设置额外请求参数
    pub fn with_extra_body(mut self, extra_body: Map<String, Value>) -> Self {
        self.extra_body = Some(extra_body);
        self
    }
}

/// Token 用量统计
#[derive(Debug, Clone)]
pub struct Usage {
    /// 提示词 token 数
    pub prompt_tokens: Option<i32>,

    /// 生成 token 数
    pub completion_tokens: Option<i32>,

    /// 总 token 数
    pub total_tokens: Option<i32>,
}

/// 对话候选结果
#[derive(Debug, Clone)]
pub struct ChatChoice {
    /// 候选序号
    pub index: i32,

    /// 候选消息
    pub message: ChatMessage,

    /// 结束原因（如 stop/length）
    pub finish_reason: Option<String>,
}

/// 对话响应
#[derive(Debug, Clone)]
pub struct ChatResponse {
    /// 实际使用的模型名称
    pub model: String,

    /// 候选结果列表
    pub choices: Vec<ChatChoice>,

    /// 用量统计
    pub usage: Option<Usage>,

    /// 请求/响应 ID
    pub id: Option<String>,
}

impl ChatResponse {
    /// 获取第一个候选结果的文本内容，若无则返回空字符串
    pub fn first_content(&self) -> &str {
        self.choices
            .first()
            .map(|c| c.message.content.as_str())
            .unwrap_or("")
    }
}

/// 流式增量内容
#[derive(Debug, Clone)]
pub struct ChatStreamDelta {
    /// 部分实现会在首个 chunk 返回 role
    pub role: Option<ChatRole>,

    /// 本次新增的文本片段
    pub content: Option<String>,
}

/// 流式候选增量
#[derive(Debug, Clone)]
pub struct ChatStreamChoiceDelta {
    /// 候选序号
    pub index: i32,

    /// 本次增量内容
    pub delta: ChatStreamDelta,

    /// 结束原因，流末尾可能出现
    pub finish_reason: Option<String>,
}

/// 对话流式分片（SSE chunk）
#[derive(Debug, Clone)]
pub struct ChatStreamChunk {
    /// 实际使用的模型名称
    pub model: String,

    /// 增量候选列表
    pub choices: Vec<ChatStreamChoiceDelta>,

    /// 部分供应商会在流末尾附带用量
    pub usage: Option<Usage>,

    /// 请求/响应 ID
    pub id: Option<String>,
}
