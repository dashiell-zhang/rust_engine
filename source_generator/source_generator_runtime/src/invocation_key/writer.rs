use crate::*;

/// 参数键生成失败
#[derive(Debug)]

pub struct InvocationKeyError {
    message: String,
}

impl InvocationKeyError {
    /// 创建参数键错误
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// 创建自定义参数键错误
    pub fn custom(message: impl Into<String>) -> Self {
        Self::new(message)
    }
}

impl Display for InvocationKeyError {
    /// 输出参数键错误描述
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for InvocationKeyError {}

/// 为调用参数提供完整且稳定的键编码
pub trait StableInvocationKey {
    /// 将当前值写入稳定调用键编码器
    fn write_stable_invocation_key(
        &self,

        writer: &mut StableInvocationKeyWriter,
    ) -> Result<(), InvocationKeyError>;
}

/// 稳定调用键的有界无歧义编码器
#[doc(hidden)]
pub struct StableInvocationKeyWriter {
    output: String,

    frames: Vec<StableInvocationKeyFrame>,

    root_count: usize,

    item_count: usize,

    failed: bool,
}

/// 稳定调用键节点写入状态
pub(crate) struct StableInvocationKeyFrame {
    expected_children: usize,

    written_children: usize,
}

impl StableInvocationKeyWriter {
    /// 稳定调用键允许的最大嵌套深度
    const MAX_DEPTH: usize = 64;

    /// 稳定调用键允许的最大节点数量
    const MAX_ITEMS: usize = 10_000;

    /// 稳定调用键允许的最大编码字节数
    const MAX_BYTES: usize = 1024 * 1024;

    /// 创建稳定调用键编码器
    pub(crate) fn new() -> Self {
        Self {
            output: "source-generator-invocation-key|".to_string(),

            frames: Vec::new(),

            root_count: 0,

            item_count: 0,

            failed: false,
        }
    }

    /// 写入包含固定数量子节点的结构节点
    pub fn write_node<F>(
        &mut self,

        tag: &str,

        child_count: usize,

        write_children: F,
    ) -> Result<(), InvocationKeyError>
    where
        F: FnOnce(&mut Self) -> Result<(), InvocationKeyError>,
    {
        let result = self.write_node_inner(tag, child_count, write_children);

        if result.is_err() {
            self.failed = true;
        }

        result
    }

    /// 写入结构节点并维护内部节点栈
    fn write_node_inner<F>(
        &mut self,

        tag: &str,

        child_count: usize,

        write_children: F,
    ) -> Result<(), InvocationKeyError>
    where
        F: FnOnce(&mut Self) -> Result<(), InvocationKeyError>,
    {
        self.begin_value()?;

        self.append("N")?;

        self.append_text(tag)?;

        self.append(";")?;

        self.append(&child_count.to_string())?;

        self.append("[")?;

        self.frames.push(StableInvocationKeyFrame {
            expected_children: child_count,

            written_children: 0,
        });

        let result = write_children(self);

        let Some(frame) = self.frames.pop() else {
            return Err(InvocationKeyError::new("稳定调用键编码器节点栈异常"));
        };

        result?;

        if frame.written_children != frame.expected_children {
            return Err(InvocationKeyError::new(format!(
                "稳定调用键节点 {tag:?} 期望 {} 个子节点，实际写入 {} 个",
                frame.expected_children, frame.written_children
            )));
        }

        self.append("]")
    }

    /// 写入带类型标签的标量节点
    pub fn write_scalar(&mut self, tag: &str, value: &str) -> Result<(), InvocationKeyError> {
        let result = self.write_scalar_inner(tag, value);

        if result.is_err() {
            self.failed = true;
        }

        result
    }

    /// 写入标量节点并维护内部预算
    fn write_scalar_inner(&mut self, tag: &str, value: &str) -> Result<(), InvocationKeyError> {
        self.begin_value()?;

        self.append("V")?;

        self.append_text(tag)?;

        self.append(";")?;

        self.append_text(value)?;

        self.append(";")
    }

    /// 完成稳定调用键编码
    pub(crate) fn finish(self) -> Result<String, InvocationKeyError> {
        if self.failed {
            return Err(InvocationKeyError::new(
                "稳定调用键写入过程中发生过错误，不能使用部分结果",
            ));
        }

        if !self.frames.is_empty() {
            return Err(InvocationKeyError::new("稳定调用键存在未结束节点"));
        }

        if self.root_count != 1 {
            return Err(InvocationKeyError::new(format!(
                "稳定调用键必须包含一个根节点，实际包含 {} 个",
                self.root_count
            )));
        }

        Ok(self.output)
    }

    /// 开始写入一个值并更新节点预算
    fn begin_value(&mut self) -> Result<(), InvocationKeyError> {
        let depth = self.frames.len();

        if depth > Self::MAX_DEPTH {
            return Err(InvocationKeyError::new(format!(
                "稳定调用键超过 {} 层深度限制",
                Self::MAX_DEPTH
            )));
        }

        self.item_count = self
            .item_count
            .checked_add(1)
            .ok_or_else(|| InvocationKeyError::new("稳定调用键节点计数溢出"))?;

        if self.item_count > Self::MAX_ITEMS {
            return Err(InvocationKeyError::new(format!(
                "稳定调用键超过 {} 个节点限制",
                Self::MAX_ITEMS
            )));
        }

        if let Some(frame) = self.frames.last_mut() {
            frame.written_children = frame
                .written_children
                .checked_add(1)
                .ok_or_else(|| InvocationKeyError::new("稳定调用键子节点计数溢出"))?;

            if frame.written_children > frame.expected_children {
                return Err(InvocationKeyError::new("稳定调用键写入了过多子节点"));
            }
        } else {
            self.root_count = self
                .root_count
                .checked_add(1)
                .ok_or_else(|| InvocationKeyError::new("稳定调用键根节点计数溢出"))?;
        }

        Ok(())
    }

    /// 写入带字节长度前缀的文本
    fn append_text(&mut self, value: &str) -> Result<(), InvocationKeyError> {
        self.append(&value.len().to_string())?;

        self.append(":")?;

        self.append(value)
    }

    /// 写入不超过总字节限制的文本
    fn append(&mut self, value: &str) -> Result<(), InvocationKeyError> {
        let next_len = self
            .output
            .len()
            .checked_add(value.len())
            .ok_or_else(|| InvocationKeyError::new("稳定调用键字节数溢出"))?;

        if next_len > Self::MAX_BYTES {
            return Err(InvocationKeyError::new(format!(
                "稳定调用键超过 {} 字节限制",
                Self::MAX_BYTES
            )));
        }

        self.output.push_str(value);

        Ok(())
    }
}
