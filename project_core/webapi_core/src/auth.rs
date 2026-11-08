use application_runtime::{auth::EcJwtCodec, context::CurrentUser};
use async_trait::async_trait;
use axum::{
    extract::{FromRequestParts, State},
    http::{Request, StatusCode, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use common::app_error::AppError as ApplicationError;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, errors::ErrorKind};
use serde_json::Value;

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
    TState: JwtDecodingState + PermissionState + TokenRenewalState + Send + Sync + 'static,
{
    let (mut parts, body) = request.into_parts();

    let user_ctx =
        match UserContext::from_request_parts(&mut parts, middleware_state.state.as_ref()).await {
            Ok(user_ctx) => user_ctx,
            Err(rejection) => return rejection.into_response(),
        };

    // 源宿主在权限判断和业务处理之前续签；拒绝访问时仍可返回续签头。
    let new_token = match middleware_state
        .state
        .try_renew_token(
            user_ctx.user_id,
            user_ctx.token_id,
            user_ctx.nbf,
            user_ctx.exp,
        )
        .await
    {
        Ok(token) => token,
        Err(error) => return AppError::from(error).into_response(),
    };
    let has_permission = middleware_state
        .state
        .check_permission(
            user_ctx.current_user(),
            &middleware_state.route_permission.module,
            &middleware_state.route_permission.route,
            middleware_state.route_permission.allow_unbound,
        )
        .await;
    let response = match has_permission {
        Err(error) => AppError::from(error).into_response(),
        Ok(false) => StatusCode::FORBIDDEN.into_response(),
        Ok(true) => {
            parts.extensions.insert(user_ctx);
            request = Request::from_parts(parts, body);
            next.run(request).await
        }
    };
    with_new_token(response, new_token.as_deref())
}

fn with_new_token(mut response: Response, token: Option<&str>) -> Response {
    if response
        .extensions()
        .get::<crate::error::UnhandledError>()
        .is_some()
    {
        return response;
    }
    if let Some(token) = token
        && let Ok(value) = axum::http::HeaderValue::from_str(token)
    {
        response.headers_mut().insert("NewToken", value);
        response.headers_mut().append(
            axum::http::header::ACCESS_CONTROL_EXPOSE_HEADERS,
            axum::http::HeaderValue::from_static("NewToken"),
        );
    }
    response
}

/// 区分认证挑战与已认证身份缺少业务声明的异常。
pub enum AuthenticationRejection {
    MissingToken,
    InvalidToken(Option<String>),
    InvalidClaims(&'static str),
}

impl IntoResponse for AuthenticationRejection {
    fn into_response(self) -> Response {
        use axum::http::{HeaderValue, header::WWW_AUTHENTICATE};
        let challenge = match self {
            Self::MissingToken => "Bearer".to_string(),
            Self::InvalidToken(description) => {
                let mut challenge = "Bearer error=\"invalid_token\"".to_string();
                if let Some(description) = description {
                    let description = description.replace('\\', "\\\\").replace('"', "\\\"");
                    challenge.push_str(&format!(", error_description=\"{description}\""));
                }
                challenge
            }
            Self::InvalidClaims(message) => return crate::error::internal_error_response(message),
        };
        let header = HeaderValue::from_str(&challenge)
            .unwrap_or_else(|_| HeaderValue::from_static("Bearer error=\"invalid_token\""));
        (StatusCode::UNAUTHORIZED, [(WWW_AUTHENTICATE, header)]).into_response()
    }
}

/// JWT 失败描述只读取所需声明，不返回 Token、密钥或底层密码学错误。
fn invalid_token(error: &ErrorKind, token: &str) -> AuthenticationRejection {
    let claims = token
        .split('.')
        .nth(1)
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or(Value::Null);
    let description = match error {
        ErrorKind::ExpiredSignature | ErrorKind::ImmatureSignature if invalid_lifetime(&claims) => {
            Some(lifetime_description(&claims))
        }
        ErrorKind::ExpiredSignature => {
            jwt_date(&claims, "exp").map(|date| format!("The token expired at '{date}'"))
        }
        ErrorKind::ImmatureSignature => {
            jwt_date(&claims, "nbf").map(|date| format!("The token is not valid before '{date}'"))
        }
        ErrorKind::InvalidIssuer => Some(format!(
            "The issuer '{}' is invalid",
            claim_text(&claims, "iss")
        )),
        ErrorKind::InvalidAudience => Some(format!(
            "The audience '{}' is invalid",
            claim_text(&claims, "aud")
        )),
        ErrorKind::MissingRequiredClaim(name) if name == "exp" => {
            Some("The token has no expiration".into())
        }
        ErrorKind::MissingRequiredClaim(name) if name == "iss" => {
            Some("The issuer '(null)' is invalid".into())
        }
        ErrorKind::MissingRequiredClaim(name) if name == "aud" => {
            Some("The audience '(null)' is invalid".into())
        }
        ErrorKind::InvalidSignature
        | ErrorKind::InvalidAlgorithm
        | ErrorKind::InvalidAlgorithmName
        | ErrorKind::UnsupportedAlgorithm => Some(
            if token.ends_with('.') {
                "The signature is invalid"
            } else {
                // 源宿主的 ECDsaSecurityKey 不指定 KeyId；尝试该密钥后仍不匹配时报告找不到签名密钥。
                "The signature key was not found"
            }
            .into(),
        ),
        _ => None,
    };
    AuthenticationRejection::InvalidToken(description)
}

fn claim_text(claims: &Value, name: &str) -> String {
    match &claims[name] {
        Value::String(value) => value.clone(),
        Value::Null => "(null)".into(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        value => value.to_string(),
    }
}

fn claim_i64(claims: &Value, name: &str) -> Option<i64> {
    let value = &claims[name];
    let value = value
        .as_array()
        .and_then(|values| values.first())
        .unwrap_or(value);
    match value {
        Value::String(value) => value.trim().parse().ok(),
        value => value.as_i64(),
    }
}

fn jwt_date(claims: &Value, name: &str) -> Option<String> {
    chrono::DateTime::from_timestamp(claim_i64(claims, name)?, 0)
        .map(|date| date.format("%m/%d/%Y %H:%M:%S").to_string())
}

fn invalid_lifetime(claims: &Value) -> bool {
    matches!((claim_i64(claims, "nbf"), claim_i64(claims, "exp")), (Some(nbf), Some(exp)) if nbf > exp)
}

fn lifetime_description(claims: &Value) -> String {
    format!(
        "The token lifetime is invalid; NotBefore: '{}', Expires: '{}'",
        jwt_date(claims, "nbf").unwrap_or_else(|| "(null)".into()),
        jwt_date(claims, "exp").unwrap_or_else(|| "(null)".into())
    )
}

/// 当前用户请求提取器
impl<S> FromRequestParts<S> for UserContext
where
    S: JwtDecodingState + Send + Sync,
{
    type Rejection = AuthenticationRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(user) = parts.extensions.get::<Self>() {
            return Ok(*user);
        }
        let authorization = common::string_helper::join_string_values(
            parts
                .headers
                .get_all("Authorization")
                .iter()
                .filter_map(|value| value.to_str().ok()),
        );
        let token = authorization
            .get(..7)
            .filter(|scheme| scheme.eq_ignore_ascii_case("Bearer "))
            .map(|_| authorization[7..].trim())
            .filter(|token| !token.is_empty())
            .ok_or(AuthenticationRejection::MissingToken)?;

        let mut validation = Validation::new(Algorithm::ES256);
        validation.validate_nbf = true;
        validation.leeway = 300;
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        validation.set_issuer(&[state.jwt_issuer()]);
        validation.set_audience(&[state.jwt_audience()]);
        // 签发模型不等于校验契约：iat 不必存在，aud 可为数组，业务 ID 可为数字声明。
        let claims = jsonwebtoken::decode::<Value>(token, state.jwt_decoding_key(), &validation)
            .map_err(|error| invalid_token(error.kind(), token))?
            .claims;
        // iss 是单值声明；不能沿用通用校验库额外接受 issuer 数组的行为。
        if !claims["iss"].is_string() {
            return Err(AuthenticationRejection::InvalidToken(None));
        }
        if invalid_lifetime(&claims) {
            return Err(AuthenticationRejection::InvalidToken(Some(
                lifetime_description(&claims),
            )));
        }
        let read_claim = |name, error| {
            claim_i64(&claims, name).ok_or(AuthenticationRejection::InvalidClaims(error))
        };
        Ok(Self {
            user_id: read_claim("userId", "当前上下文中无有效的 UserId")?,
            token_id: read_claim("tokenId", "当前上下文中无有效的 tokenId")?,
            nbf: read_claim("nbf", "当前上下文中无有效的 nbf")?,
            exp: read_claim("exp", "当前上下文中无有效的 exp")?,
        })
    }
}

/// 可选当前用户：无凭据或认证失败时保持匿名，已认证但业务声明损坏则报告异常。
impl<S> FromRequestParts<S> for OptionalUserContext
where
    S: JwtDecodingState + Send + Sync,
{
    type Rejection = AuthenticationRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match UserContext::from_request_parts(parts, state).await {
            Ok(user) => Ok(Self(Some(user))),
            Err(
                AuthenticationRejection::MissingToken | AuthenticationRejection::InvalidToken(_),
            ) => Ok(Self(None)),
            Err(error) => Err(error),
        }
    }
}
