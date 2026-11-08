/// 应用层当前用户上下文
#[derive(Clone, Copy)]
pub struct CurrentUser {
    /// 当前用户 ID
    pub user_id: i64,
    /// 当前 Token ID
    pub token_id: Option<i64>,
}
