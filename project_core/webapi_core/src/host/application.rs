use super::state::AppState;
use application_interface::error::AppError;
use application_runtime::context::CurrentUser;
use async_trait::async_trait;

/// 公共 HTTP 宿主需要的业务策略，由运行入口显式装配。
#[async_trait]
pub trait WebApiApplication: Send + Sync {
    /// 路由发布前验证入口要求的资源，避免把装配错误延迟到请求阶段。
    fn validate_resources(
        &self,
        _resources: &super::resources::WebApiResources,
    ) -> Result<(), String> {
        Ok(())
    }
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
