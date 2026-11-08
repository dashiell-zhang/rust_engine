//! 雪花 ID 的持久化位布局。分区边界允许落在自定义纪元之前。
pub const EPOCH_MILLISECONDS: i64 = 1_640_995_200_000;
pub const MACHINE_ID_BITS: u8 = 5;
pub const DATA_CENTER_ID_BITS: u8 = 5;
pub const SEQUENCE_BITS: u8 = 11;
pub const TIMESTAMP_LEFT_SHIFT: u8 = SEQUENCE_BITS + MACHINE_ID_BITS + DATA_CENTER_ID_BITS;

/// 计算完整毫秒的最小 ID，拒绝溢出，避免边界绕回而覆盖其他时间。
pub fn min_id_at_unix_millis(milliseconds: i64) -> Result<i64, &'static str> {
    milliseconds
        .checked_sub(EPOCH_MILLISECONDS)
        .and_then(|value| value.checked_mul(1i64 << TIMESTAMP_LEFT_SHIFT))
        .ok_or("时间超出雪花 ID 的有符号范围")
}

pub fn unix_millis_from_id(id: i64) -> i64 {
    (id >> TIMESTAMP_LEFT_SHIFT) + EPOCH_MILLISECONDS
}

pub fn is_millisecond_boundary(id: i64) -> bool {
    id & ((1i64 << TIMESTAMP_LEFT_SHIFT) - 1) == 0
}
