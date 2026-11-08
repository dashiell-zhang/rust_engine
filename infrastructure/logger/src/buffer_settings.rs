use serde::Deserialize;

#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct LogBufferSettings {
    pub queue_capacity: usize,
    pub max_buffered_bytes: u64,
    pub important_queue_capacity: usize,
    pub important_max_buffered_bytes: u64,
    pub batch_size: usize,
    pub flush_interval_milliseconds: u64,
}

impl Default for LogBufferSettings {
    fn default() -> Self {
        Self {
            queue_capacity: 8192,
            max_buffered_bytes: 48 * 1024 * 1024,
            important_queue_capacity: 2048,
            important_max_buffered_bytes: 16 * 1024 * 1024,
            batch_size: 1000,
            flush_interval_milliseconds: 1000,
        }
    }
}

impl LogBufferSettings {
    pub fn validate(&self) -> Result<(), String> {
        if [
            self.queue_capacity,
            self.important_queue_capacity,
            self.batch_size,
        ]
        .into_iter()
        .any(|n| n == 0 || n > i32::MAX as usize)
        {
            return Err("日志队列容量和批量大小必须为正的 i32 整数".into());
        }
        if [self.max_buffered_bytes, self.important_max_buffered_bytes]
            .into_iter()
            .any(|n| n == 0 || n > i64::MAX as u64)
        {
            return Err("日志缓冲字节预算必须为正的 i64 整数".into());
        }
        if !(1..=4_294_967_294).contains(&self.flush_interval_milliseconds) {
            return Err("日志落盘间隔必须介于 1 和 4294967294 毫秒之间".into());
        }
        Ok(())
    }
}
