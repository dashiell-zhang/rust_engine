use super::state::AppState;
use application_interface::{context::CurrentUser, error::AppError};
use async_trait::async_trait;

/// 公共 HTTP 宿主需要的业务策略，由运行入口显式装配。
#[async_trait]
pub trait WebApiApplication: Send + Sync {
    async fn try_renew_token(
        &self,
        state: &AppState,
        user_id: i64,
        token_id: i64,
        nbf: i64,
        exp: i64,
    ) -> Result<Option<String>, AppError>;
    async fn check_permission(
        &self,
        state: &AppState,
        current_user: CurrentUser,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<bool, AppError>;
    async fn sync_permission_routes(&self, state: &AppState) -> Result<bool, AppError>;
}
