use application_interface::{auth::EcJwtCodec, context::CurrentUser};
use application_model::authorize::models::JwtClaims;
use async_trait::async_trait;
use axum::{
    extract::{FromRequestParts, State},
    http::{Request, StatusCode, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use common::app_error::AppError as ApplicationError;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};

use crate::error::AppError;

/// JWT 验签状态访问接口
pub trait JwtDecodingState: Send + Sync {
    /// 获取 JWT 验签密钥
    fn jwt_decoding_key(&self) -> &DecodingKey;

    /// 获取 JWT 颁发者
    fn jwt_issuer(&self) -> &str;

    /// 获取 JWT 受众
    fn jwt_audience(&self) -> &str;
}

/// JWT 编解码器状态访问接口
pub trait HasJwtCodec: Send + Sync {
    /// 获取 JWT 编解码器
    fn jwt_codec(&self) -> &EcJwtCodec;
}

/// 令牌有效性校验状态接口
#[async_trait]
pub trait TokenValidationState: Send + Sync {
    /// 校验令牌是否仍然有效
    async fn validate_token(&self, user_id: i64, token_id: i64) -> Result<bool, ApplicationError>;
}

/// 共享状态 JWT 验签适配
impl<T> JwtDecodingState for std::sync::Arc<T>
where
    T: JwtDecodingState,
{
    /// 获取 JWT 验签密钥
    fn jwt_decoding_key(&self) -> &DecodingKey {
        self.as_ref().jwt_decoding_key()
    }

    /// 获取 JWT 颁发者
    fn jwt_issuer(&self) -> &str {
        self.as_ref().jwt_issuer()
    }

    /// 获取 JWT 受众
    fn jwt_audience(&self) -> &str {
        self.as_ref().jwt_audience()
    }
}

/// 共享状态 JWT 编解码器适配
impl<T> HasJwtCodec for std::sync::Arc<T>
where
    T: HasJwtCodec,
{
    /// 获取 JWT 编解码器
    fn jwt_codec(&self) -> &EcJwtCodec {
        self.as_ref().jwt_codec()
    }
}

/// 共享状态令牌校验适配
#[async_trait]
impl<T> TokenValidationState for std::sync::Arc<T>
where
    T: TokenValidationState,
{
    /// 校验令牌是否仍然有效
    async fn validate_token(&self, user_id: i64, token_id: i64) -> Result<bool, ApplicationError> {
        self.as_ref().validate_token(user_id, token_id).await
    }
}

/// Token 续签能力状态接口，由宿主状态实现并注入到权限中间件
#[async_trait]
pub trait TokenRenewalState: Send + Sync {
    /// 尝试为当前 Token 续签，剩余有效期不足总有效期一半时返回新 Token，否则返回 None
    async fn try_renew_token(
        &self,
        user_id: i64,
        token_id: i64,
        nbf: i64,
        exp: i64,
    ) -> Result<Option<String>, ApplicationError>;
}

/// 共享状态 Token 续签适配，将 Arc 包装透传给内部实现
#[async_trait]
impl<T> TokenRenewalState for std::sync::Arc<T>
where
    T: TokenRenewalState,
{
    /// 委托内部状态执行 Token 续签
    async fn try_renew_token(
        &self,
        user_id: i64,
        token_id: i64,
        nbf: i64,
        exp: i64,
    ) -> Result<Option<String>, ApplicationError> {
        self.as_ref()
            .try_renew_token(user_id, token_id, nbf, exp)
            .await
    }
}

/// ECDSA JWT 编解码器验签适配
impl JwtDecodingState for EcJwtCodec {
    /// 获取 JWT 验签密钥
    fn jwt_decoding_key(&self) -> &DecodingKey {
        self.decoding_key()
    }

    /// 获取 JWT 颁发者
    fn jwt_issuer(&self) -> &str {
        ""
    }

    /// 获取 JWT 受众
    fn jwt_audience(&self) -> &str {
        ""
    }
}

/// 当前登录用户上下文
#[derive(Clone, Copy)]
pub struct UserContext {
    /// 当前用户 ID
    pub user_id: i64,

    /// 当前令牌 ID
    pub token_id: i64,

    /// Token 签发时间，Unix 秒级时间戳，对应 JWT nbf 字段
    pub nbf: i64,

    /// Token 过期时间，Unix 秒级时间戳，对应 JWT exp 字段
    pub exp: i64,
}

impl UserContext {
    /// 转换为应用层当前用户上下文
    pub fn current_user(&self) -> CurrentUser {
        CurrentUser {
            user_id: self.user_id,
            token_id: Some(self.token_id),
        }
    }
}

/// 可选当前登录用户上下文
#[derive(Clone, Copy)]
pub struct OptionalUserContext(pub Option<UserContext>);

impl OptionalUserContext {
    /// 获取当前用户上下文
    pub fn current_user(&self) -> Option<CurrentUser> {
        self.0.map(|item| item.current_user())
    }
}

/// 路由权限元数据
#[derive(Clone)]
pub struct RoutePermissionMetadata {
    /// 权限模块名
    pub module: String,

    /// 权限路由名
    pub route: String,

    /// 未绑定功能时是否允许访问
    pub allow_unbound: bool,
}

impl RoutePermissionMetadata {
    /// 创建路由权限元数据
    pub fn new(module: impl Into<String>, route: impl Into<String>, allow_unbound: bool) -> Self {
        Self {
            module: module.into(),
            route: route.into(),
            allow_unbound,
        }
    }
}

/// 路由权限校验状态接口
#[async_trait]
pub trait PermissionState: Send + Sync {
    /// 校验当前用户是否具备指定路由权限
    async fn check_permission(
        &self,
        current_user: CurrentUser,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<bool, ApplicationError>;
}

/// 共享状态权限校验适配
#[async_trait]
impl<T> PermissionState for std::sync::Arc<T>
where
    T: PermissionState,
{
    /// 校验当前用户是否具备指定路由权限
    async fn check_permission(
        &self,
        current_user: CurrentUser,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<bool, ApplicationError> {
        self.as_ref()
            .check_permission(current_user, module, route, allow_unbound)
            .await
    }
}

/// 路由权限中间件状态
pub struct PermissionMiddlewareState<TState> {
    /// 应用状态
    pub state: std::sync::Arc<TState>,

    /// 路由权限元数据
    pub route_permission: RoutePermissionMetadata,
}

impl<TState> Clone for PermissionMiddlewareState<TState> {
    /// 克隆路由权限中间件状态
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            route_permission: self.route_permission.clone(),
        }
    }
}

/// 路由权限校验中间件
pub async fn permission_middleware<TState>(
    State(middleware_state): State<PermissionMiddlewareState<TState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response
where
    TState: JwtDecodingState
        + TokenValidationState
        + PermissionState
        + TokenRenewalState
        + Send
        + Sync
        + 'static,
{
    let (mut parts, body) = request.into_parts();

    let user_ctx =
        match UserContext::from_request_parts(&mut parts, middleware_state.state.as_ref()).await {
            Ok(user_ctx) => user_ctx,
            Err((status_code, message)) => return (status_code, message).into_response(),
        };

    let has_permission = match middleware_state
        .state
        .check_permission(
            user_ctx.current_user(),
            &middleware_state.route_permission.module,
            &middleware_state.route_permission.route,
            middleware_state.route_permission.allow_unbound,
        )
        .await
    {
        Ok(has_permission) => has_permission,
        Err(error) => return AppError::from(error).into_response(),
    };

    if !has_permission {
        return AppError::from(ApplicationError::unauthorized("当前账户无权访问此接口"))
            .into_response();
    }

    parts.extensions.insert(user_ctx);
    request = Request::from_parts(parts, body);

    let mut response = next.run(request).await;

    // Token 剩余有效期不足一半时触发续签，将新 Token 写入响应头供客户端替换
    if let Ok(Some(new_token)) = middleware_state
        .state
        .try_renew_token(
            user_ctx.user_id,
            user_ctx.token_id,
            user_ctx.nbf,
            user_ctx.exp,
        )
        .await
        && let Ok(header_value) = axum::http::HeaderValue::from_str(&new_token)
    {
        response.headers_mut().insert("X-New-Token", header_value);
    }

    response
}

/// 当前用户请求提取器
impl<S> FromRequestParts<S> for UserContext
where
    S: JwtDecodingState + TokenValidationState + Send + Sync,
{
    type Rejection = (StatusCode, String);

    /// 从请求头解析当前用户
    fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let user_ctx = parts.extensions.get::<Self>().copied();

        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let jwt_decoding_key = state.jwt_decoding_key().clone();
        let jwt_issuer = state.jwt_issuer().to_string();
        let jwt_audience = state.jwt_audience().to_string();
        let token_validation_state = state;

        async move {
            if let Some(user_ctx) = user_ctx {
                return Ok(user_ctx);
            }

            let auth_header = auth_header.ok_or((
                StatusCode::UNAUTHORIZED,
                "缺少 Authorization 请求头".to_string(),
            ))?;

            let token = auth_header.strip_prefix("Bearer ").ok_or((
                StatusCode::UNAUTHORIZED,
                "Authorization 格式错误，应为 Bearer <token>".to_string(),
            ))?;

            let mut validation = Validation::new(Algorithm::ES256);
            validation.set_issuer(&[jwt_issuer.as_str()]);
            validation.set_audience(&[jwt_audience.as_str()]);

            let token_data =
                jsonwebtoken::decode::<JwtClaims>(token, &jwt_decoding_key, &validation)
                    .map_err(|_| (StatusCode::UNAUTHORIZED, "Token 无效或已过期".to_string()))?;

            let user_id = token_data
                .claims
                .user_id
                .parse::<i64>()
                .map_err(|_| (StatusCode::UNAUTHORIZED, "Token 中 userId 无效".to_string()))?;

            let token_id = token_data.claims.token_id.parse::<i64>().map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    "Token 中 tokenId 无效".to_string(),
                )
            })?;

            let is_valid = token_validation_state
                .validate_token(user_id, token_id)
                .await
                .map_err(|error| (StatusCode::UNAUTHORIZED, error.message().to_string()))?;

            if !is_valid {
                return Err((StatusCode::UNAUTHORIZED, "Token 已失效".to_string()));
            }

            // 提取 nbf 和 exp 用于后续续签条件判断
            let nbf = token_data.claims.nbf;
            let exp = token_data.claims.exp;

            Ok(Self {
                user_id,
                token_id,
                nbf,
                exp,
            })
        }
    }
}

/// 可选当前用户请求提取器
impl<S> FromRequestParts<S> for OptionalUserContext
where
    S: JwtDecodingState + TokenValidationState + Send + Sync,
{
    type Rejection = (StatusCode, String);

    /// 从请求头解析可选当前用户
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if parts.headers.get("Authorization").is_none() {
            return Ok(Self(None));
        }

        UserContext::from_request_parts(parts, state)
            .await
            .map(|user_ctx| Self(Some(user_ctx)))
    }
}
