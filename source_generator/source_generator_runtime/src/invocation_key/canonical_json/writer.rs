use crate::*;

/// 限制参数 JSON 序列化期间允许写入的最大字节数
pub(crate) struct LimitedJsonWriter {
    buffer: Vec<u8>,

    max_bytes: usize,

    exceeded: bool,
}

impl LimitedJsonWriter {
    /// 创建有界 JSON 写入器
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),

            max_bytes,

            exceeded: false,
        }
    }

    /// 返回是否因为字节限制拒绝了写入
    pub(crate) fn exceeded(&self) -> bool {
        self.exceeded
    }

    /// 获取已经写入的 JSON 字节
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }
}

impl Write for LimitedJsonWriter {
    /// 写入不超过配置上限的 JSON 字节
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next_len) = self.buffer.len().checked_add(bytes.len()) else {
            self.exceeded = true;

            return Err(io::Error::other("参数 JSON 字节数溢出"));
        };

        if next_len > self.max_bytes {
            self.exceeded = true;

            return Err(io::Error::other("参数 JSON 超过字节限制"));
        }

        self.buffer.extend_from_slice(bytes);

        Ok(bytes.len())
    }

    /// 刷新内存写入器
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
