use std::sync::{Arc, Mutex};

use common::app_error::AppError;
use crc::{CRC_64_XZ, Crc, Digest};
use reqwest::header::HeaderMap;

// OSS 使用 ECMA 多项式的反射算法，并在计算前后取反，参数对应 CRC-64/XZ。
// 不能改用同名多项式但初始值及反射规则不同的 CRC_64_ECMA_182。
const OSS_CRC64: Crc<u64> = Crc::<u64>::new(&CRC_64_XZ);

/// 跟踪实际传输的字节；上传流与响应检查共享摘要，不预读整个文件。
#[derive(Clone)]
pub(super) struct TransferChecksum(Arc<Mutex<Digest<'static, u64>>>);

impl TransferChecksum {
    pub(super) fn new() -> Self {
        Self(Arc::new(Mutex::new(OSS_CRC64.digest())))
    }

    pub(super) fn update(&self, bytes: &[u8]) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .update(bytes);
    }

    pub(super) fn verify(&self, headers: &HeaderMap) -> Result<(), AppError> {
        let Some(expected) = headers.get("x-oss-hash-crc64ecma") else {
            return Ok(());
        };
        let actual = self
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .finalize()
            .to_string();
        if actual.as_bytes() != expected.as_bytes() {
            return Err(AppError::internal("阿里云 OSS 文件 CRC64 校验失败"));
        }
        Ok(())
    }
}
