#![allow(clippy::collapsible_if)]

use async_recursion::async_recursion;
use chrono::{Duration, Utc};
use common::{
    crypto_helper::{base64_encode, hash_password_pbkdf2_sha256, verify_password_pbkdf2_sha256},
    img_helper,
    string_helper::trim_prefix_name,
};
use distributed_cache::{CacheEntryOptions, DistributedCacheExt};
use indexmap::IndexMap;
use repository::extensions::SoftDeleteQueryExt;
use repository::{
    app_setting, function, function_authorize, function_route, user, user_bind_external, user_role,
    user_token,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, IntoActiveModel,
    QueryFilter, QueryOrder, Set, prelude::DateTimeWithTimeZone, sea_query::Condition,
};
use serde_json::{self, json};
use source_generator_runtime::retry;
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration as StdDuration, Instant},
};
use uuid::Uuid;

use crate::{
    task_center::{models::CreateQueueTaskDto, service::TaskCenterService},
    write_support::{
        apply_update_user_audit_field, build_system_create_audit_fields, write_data_update_log,
    },
};
use application_interface::{
    context::{AuthorizeContext, CurrentUser},
    error::AppError,
};
use application_model::{
    authorize::models::{
        FunctionRouteDto, FunctionTreeDto, GetFunctionRouteListRequest, GetTokenBySmsRequest,
        GetTokenByWxAppRequest, GetTokenByWxMiniAppRequest, GetTokenRequest,
        GetWeiXinAppUserInfoDto, JwtClaims, SendSmsVerifyCodeRequest, SetFunctionRouteDto,
        UpdatePasswordByOldPasswordRequest, UpdatePasswordBySmsRequest,
    },
    message::models::SendSmsMessage,
};

/// 模块类型
const FUNCTION_TYPE_MODULE: i32 = 1;

/// 功能类型
const FUNCTION_TYPE_FUNCTION: i32 = 2;

/// 路由权限缓存版本键
const AUTHORIZE_ROUTE_VERSION_KEY: &str = "authorize:route:version";

/// 授权版本进程内快照缓存时长（毫秒）
const AUTHORIZE_VERSION_LOCAL_TTL_MILLIS: u64 = 3_000;

/// Token 有效性缓存时长（秒）
const TOKEN_VALID_CACHE_TTL_SECONDS: u64 = 60;

#[derive(Clone, Copy)]
struct LocalAuthorizeVersionCacheItem {
    value: u64,
    expires_at: Instant,
}

static ROUTE_AUTHORIZE_VERSION_LOCAL_CACHE: OnceLock<
    Mutex<Option<LocalAuthorizeVersionCacheItem>>,
> = OnceLock::new();
static USER_AUTHORIZE_VERSION_LOCAL_CACHE: OnceLock<
    Mutex<HashMap<i64, LocalAuthorizeVersionCacheItem>>,
> = OnceLock::new();

fn route_authorize_version_local_cache() -> &'static Mutex<Option<LocalAuthorizeVersionCacheItem>> {
    ROUTE_AUTHORIZE_VERSION_LOCAL_CACHE.get_or_init(|| Mutex::new(None))
}

fn user_authorize_version_local_cache()
-> &'static Mutex<HashMap<i64, LocalAuthorizeVersionCacheItem>> {
    USER_AUTHORIZE_VERSION_LOCAL_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn authorize_version_local_expiry() -> Instant {
    Instant::now() + StdDuration::from_millis(AUTHORIZE_VERSION_LOCAL_TTL_MILLIS)
}

fn try_get_route_authorize_version_local() -> Option<u64> {
    let mut guard = route_authorize_version_local_cache().lock().ok()?;

    match *guard {
        Some(item) if item.expires_at > Instant::now() => Some(item.value),
        Some(_) => {
            *guard = None;
            None
        }
        None => None,
    }
}

fn set_route_authorize_version_local(version: u64) {
    if let Ok(mut guard) = route_authorize_version_local_cache().lock() {
        *guard = Some(LocalAuthorizeVersionCacheItem {
            value: version,
            expires_at: authorize_version_local_expiry(),
        });
    }
}

fn try_get_user_authorize_version_local(user_id: i64) -> Option<u64> {
    let mut guard = user_authorize_version_local_cache().lock().ok()?;
    let item = guard.get(&user_id).copied()?;

    if item.expires_at > Instant::now() {
        Some(item.value)
    } else {
        guard.remove(&user_id);
        None
    }
}

fn set_user_authorize_version_local(user_id: i64, version: u64) {
    if let Ok(mut guard) = user_authorize_version_local_cache().lock() {
        guard.insert(
            user_id,
            LocalAuthorizeVersionCacheItem {
                value: version,
                expires_at: authorize_version_local_expiry(),
            },
        );
    }
}

/// 授权模块应用服务
pub struct AuthorizeService<'a> {
    /// 授权模块上下文
    context: AuthorizeContext<'a>,
}

impl<'a> AuthorizeService<'a> {
    /// 创建授权模块应用服务
    pub fn new(context: AuthorizeContext<'a>) -> Self {
        Self { context }
    }

    /// 校验登录请求
    fn validate_get_token_request(request: &GetTokenRequest) -> Result<(), AppError> {
        if request.user_name.trim().is_empty() {
            return Err(AppError::bad_request("用户名不可以空"));
        }

        if request.password.is_empty() {
            return Err(AppError::bad_request("密码不可以空"));
        }

        Ok(())
    }

    /// 校验短信登录请求
    fn validate_get_token_by_sms_request(request: &GetTokenBySmsRequest) -> Result<(), AppError> {
        if request.phone.trim().is_empty() {
            return Err(AppError::bad_request("手机号不可以空"));
        }

        if request.verify_code.trim().is_empty() {
            return Err(AppError::bad_request("短信验证码不可以空"));
        }

        Ok(())
    }

    /// 校验短信验证码发送请求
    fn validate_send_sms_verify_code_request(
        request: &SendSmsVerifyCodeRequest,
    ) -> Result<(), AppError> {
        if request.phone.trim().is_empty() {
            return Err(AppError::bad_request("手机号不可以空"));
        }

        Ok(())
    }

    /// 校验短信修改密码请求
    fn validate_update_password_by_sms_request(
        request: &UpdatePasswordBySmsRequest,
    ) -> Result<(), AppError> {
        if request.sms_code.trim().is_empty() {
            return Err(AppError::bad_request("短信验证码不可以空"));
        }

        if request.new_password.is_empty() {
            return Err(AppError::bad_request("新密码不可以空"));
        }

        Ok(())
    }

    /// 校验旧密码修改密码请求
    fn validate_update_password_by_old_password_request(
        request: &UpdatePasswordByOldPasswordRequest,
    ) -> Result<(), AppError> {
        if request.old_password.is_empty() {
            return Err(AppError::bad_request("旧密码不可以空"));
        }

        if request.new_password.is_empty() {
            return Err(AppError::bad_request("新密码不可以空"));
        }

        Ok(())
    }

    /// 用户名密码登录并签发 Token
    pub async fn get_token(&self, request: GetTokenRequest) -> Result<String, AppError> {
        Self::validate_get_token_request(&request)?;

        let user_models = user::Entity::find()
            .filter(user::Column::UserName.eq(request.user_name.trim()))
            .not_deleted(user::Column::DeleteTime)
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户查询失败: {error}")))?;

        let Some(user_model) = user_models.into_iter().find(|user_model| {
            verify_password_pbkdf2_sha256(
                &request.password,
                &user_model.id.to_string(),
                &user_model.password,
            )
        }) else {
            return Err(AppError::unauthorized("账号或密码错误"));
        };

        self.get_token_by_user_id(user_model.id, None).await
    }

    /// 手机号短信验证码登录并签发 Token
    pub async fn get_token_by_sms(
        &self,
        request: GetTokenBySmsRequest,
    ) -> Result<String, AppError> {
        Self::validate_get_token_by_sms_request(&request)?;

        // 先消费验证码 再进入用户查询和自动注册流程
        self.verify_phone_code(request.phone.trim(), request.verify_code.trim())
            .await?;

        let existing_user = user::Entity::find()
            .filter(user::Column::Phone.eq(request.phone.trim()))
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户查询失败: {error}")))?;

        let user_id = if let Some(existing_user) = existing_user {
            existing_user.id
        } else {
            // 短信首登时自动补齐一条最小可用用户记录
            let user_id = self
                .context
                .app
                .id_generator
                .get_id()
                .await
                .map_err(|error| {
                    AppError::internal(format!("生成短信登录用户主键失败: {error}"))
                })?;
            let audit_fields = build_system_create_audit_fields();
            let user_name = format!("sms_user_{user_id}");
            let display_name = format!("短信用户{user_id}");
            let password = hash_password_pbkdf2_sha256(
                &base64_encode(&user_id.to_string()),
                &user_id.to_string(),
            );

            user::ActiveModel {
                id: Set(user_id),
                user_name: Set(user_name),
                name: Set(display_name),
                phone: Set(request.phone.trim().to_string()),
                email: Set(None),
                password: Set(password),
                create_time: Set(audit_fields.create_time),
                delete_time: Set(audit_fields.delete_time),
                update_time: Set(Some(audit_fields.create_time)),
                create_user_id: Set(None),
                update_user_id: Set(None),
                delete_user_id: Set(None),
            }
            .insert(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("短信登录用户创建失败: {error}")))?;

            // 记录短信首登自动注册产生的用户写入
            write_data_update_log(
                self.context.app,
                "User",
                user_id,
                "auto_register_by_sms",
                None,
                Some(json!({
                    "id": user_id,
                    "user_name": format!("sms_user_{user_id}"),
                    "name": format!("短信用户{user_id}"),
                    "register_source": "sms",
                })),
                None,
            )
            .await?;

            user_id
        };

        self.get_token_by_user_id(user_id, None).await
    }

    /// 微信小程序登录并签发 Token
    ///
    /// 用 AppId + Code 换取 openId，再按 openId 匹配或自动注册用户
    pub async fn get_token_by_wx_mini_app(
        &self,
        request: GetTokenByWxMiniAppRequest,
    ) -> Result<String, AppError> {
        let app_id = request.app_id.trim().to_string();
        let code = request.code.trim().to_string();

        if app_id.is_empty() {
            return Err(AppError::bad_request("AppId 不可为空"));
        }
        if code.is_empty() {
            return Err(AppError::bad_request("Code 不可为空"));
        }

        // 先向微信换取 open_id 再做本地绑定查找
        let open_id = self.get_wx_mini_app_open_id(&app_id, &code).await?;

        // 查询已有绑定记录
        let existing_bind = user_bind_external::Entity::find()
            .filter(user_bind_external::Column::AppName.eq("WeiXinMiniApp"))
            .filter(user_bind_external::Column::AppId.eq(&app_id))
            .filter(user_bind_external::Column::OpenId.eq(&open_id))
            .not_deleted(user_bind_external::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("微信绑定记录查询失败: {error}")))?;

        let user_id = if let Some(bind) = existing_bind {
            bind.user_id
        } else {
            let bind_lock_key = format!("authorize:wx-mini-app-bind:{app_id}:{open_id}");
            let bind_lock_guard = self
                .context
                .authorize_lock
                .try_lock(&bind_lock_key, std::time::Duration::from_secs(10), 1)
                .await?;

            let Some(_bind_lock_guard) = bind_lock_guard else {
                return Err(AppError::conflict("微信登录处理中，请稍后重试"));
            };

            let existing_bind = user_bind_external::Entity::find()
                .filter(user_bind_external::Column::AppName.eq("WeiXinMiniApp"))
                .filter(user_bind_external::Column::AppId.eq(&app_id))
                .filter(user_bind_external::Column::OpenId.eq(&open_id))
                .not_deleted(user_bind_external::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("微信绑定记录查询失败: {error}")))?;

            if let Some(bind) = existing_bind {
                bind.user_id
            } else {
                // 自动注册新用户并绑定
                let new_user_id =
                    self.context
                        .app
                        .id_generator
                        .get_id()
                        .await
                        .map_err(|error| {
                            AppError::internal(format!("生成微信小程序用户主键失败: {error}"))
                        })?;
                let audit_fields = build_system_create_audit_fields();
                let user_name = format!("wx_user_{new_user_id}");
                let display_name = format!("微信用户{new_user_id}");
                let password = hash_password_pbkdf2_sha256(
                    &base64_encode(&new_user_id.to_string()),
                    &new_user_id.to_string(),
                );

                user::ActiveModel {
                    id: Set(new_user_id),
                    user_name: Set(user_name),
                    name: Set(display_name.clone()),
                    phone: Set(String::new()),
                    email: Set(None),
                    password: Set(password),
                    create_time: Set(audit_fields.create_time),
                    delete_time: Set(audit_fields.delete_time),
                    update_time: Set(Some(audit_fields.create_time)),
                    create_user_id: Set(None),
                    update_user_id: Set(None),
                    delete_user_id: Set(None),
                }
                .insert(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("微信新用户创建失败: {error}")))?;

                // 记录微信小程序首登自动注册产生的用户写入
                write_data_update_log(
                    self.context.app,
                    "User",
                    new_user_id,
                    "auto_register_by_wx_mini_app",
                    None,
                    Some(json!({
                        "id": new_user_id,
                        "user_name": format!("wx_user_{new_user_id}"),
                        "name": format!("微信用户{new_user_id}"),
                        "register_source": "wx_mini_app",
                        "app_id": app_id,
                    })),
                    None,
                )
                .await?;

                let bind_id = self
                    .context
                    .app
                    .id_generator
                    .get_id()
                    .await
                    .map_err(|error| {
                        AppError::internal(format!("生成微信小程序绑定主键失败: {error}"))
                    })?;

                user_bind_external::ActiveModel {
                    id: Set(bind_id),
                    user_id: Set(new_user_id),
                    app_name: Set("WeiXinMiniApp".to_string()),
                    app_id: Set(app_id.clone()),
                    open_id: Set(open_id.clone()),
                    create_time: Set(audit_fields.create_time),
                    delete_time: Set(audit_fields.delete_time),
                }
                .insert(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("微信绑定记录创建失败: {error}")))?;

                new_user_id
            }
        };

        self.get_token_by_user_id(user_id, None).await
    }

    /// 微信公众号网页登录并签发 Token
    pub async fn get_token_by_wx_app(
        &self,
        request: GetTokenByWxAppRequest,
    ) -> Result<String, AppError> {
        let app_id = request.app_id.trim().to_string();
        let code = request.code.trim().to_string();

        if app_id.is_empty() {
            return Err(AppError::bad_request("AppId 不可为空"));
        }

        if code.is_empty() {
            return Err(AppError::bad_request("Code 不可为空"));
        }

        let (access_token, open_id) = self
            .get_wx_app_access_token_and_open_id(&app_id, &code)
            .await?;

        // 公众号网页登录需要额外拉取用户资料 才能拿到昵称等注册信息
        let user_info = self.get_wx_app_user_info(&access_token, &open_id).await?;
        let bind_open_id = user_info
            .open_id
            .as_deref()
            .filter(|item| !item.is_empty())
            .unwrap_or(&open_id)
            .to_string();

        let existing_bind = user_bind_external::Entity::find()
            .filter(user_bind_external::Column::AppName.eq("WeiXinApp"))
            .filter(user_bind_external::Column::AppId.eq(&app_id))
            .filter(user_bind_external::Column::OpenId.eq(&bind_open_id))
            .not_deleted(user_bind_external::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("微信绑定记录查询失败: {error}")))?;

        let user_id = if let Some(bind) = existing_bind {
            bind.user_id
        } else {
            let bind_lock_key = format!("authorize:wx-app-bind:{app_id}:{bind_open_id}");
            let bind_lock_guard = self
                .context
                .authorize_lock
                .try_lock(&bind_lock_key, std::time::Duration::from_secs(10), 1)
                .await?;

            let Some(_bind_lock_guard) = bind_lock_guard else {
                return Err(AppError::conflict("微信登录处理中，请稍后重试"));
            };

            let existing_bind = user_bind_external::Entity::find()
                .filter(user_bind_external::Column::AppName.eq("WeiXinApp"))
                .filter(user_bind_external::Column::AppId.eq(&app_id))
                .filter(user_bind_external::Column::OpenId.eq(&bind_open_id))
                .not_deleted(user_bind_external::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("微信绑定记录查询失败: {error}")))?;

            if let Some(bind) = existing_bind {
                bind.user_id
            } else {
                // 首次登录时自动补齐最小可用用户并建立公众号绑定关系
                let new_user_id =
                    self.context
                        .app
                        .id_generator
                        .get_id()
                        .await
                        .map_err(|error| {
                            AppError::internal(format!("生成微信公众号用户主键失败: {error}"))
                        })?;
                let audit_fields = build_system_create_audit_fields();
                let display_name = user_info
                    .nick_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("微信用户{new_user_id}"));
                let user_name = format!("wx_app_user_{new_user_id}");
                let password = hash_password_pbkdf2_sha256(
                    &base64_encode(&new_user_id.to_string()),
                    &new_user_id.to_string(),
                );

                user::ActiveModel {
                    id: Set(new_user_id),
                    user_name: Set(user_name),
                    name: Set(display_name.clone()),
                    phone: Set(String::new()),
                    email: Set(None),
                    password: Set(password),
                    create_time: Set(audit_fields.create_time),
                    delete_time: Set(audit_fields.delete_time),
                    update_time: Set(Some(audit_fields.create_time)),
                    create_user_id: Set(None),
                    update_user_id: Set(None),
                    delete_user_id: Set(None),
                }
                .insert(self.context.app.db)
                .await
                .map_err(|error| {
                    AppError::internal(format!("微信公众号新用户创建失败: {error}"))
                })?;

                // 记录微信公众号首登自动注册产生的用户写入
                write_data_update_log(
                    self.context.app,
                    "User",
                    new_user_id,
                    "auto_register_by_wx_app",
                    None,
                    Some(json!({
                        "id": new_user_id,
                        "user_name": format!("wx_app_user_{new_user_id}"),
                        "name": display_name,
                        "register_source": "wx_app",
                        "app_id": app_id,
                    })),
                    None,
                )
                .await?;

                let bind_id = self
                    .context
                    .app
                    .id_generator
                    .get_id()
                    .await
                    .map_err(|error| {
                        AppError::internal(format!("生成微信公众号绑定主键失败: {error}"))
                    })?;

                user_bind_external::ActiveModel {
                    id: Set(bind_id),
                    user_id: Set(new_user_id),
                    app_name: Set("WeiXinApp".to_string()),
                    app_id: Set(app_id),
                    open_id: Set(bind_open_id),
                    create_time: Set(audit_fields.create_time),
                    delete_time: Set(audit_fields.delete_time),
                }
                .insert(self.context.app.db)
                .await
                .map_err(|error| {
                    AppError::internal(format!("微信公众号绑定记录创建失败: {error}"))
                })?;

                new_user_id
            }
        };

        self.get_token_by_user_id(user_id, None).await
    }

    /// 通过 AppId + Code 获取微信小程序 openId
    ///
    /// 从 AppSetting 表读取对应 AppSecret，调用微信 jscode2session 接口
    #[retry(max_attempts = 3, delay_ms = 200)]
    async fn get_wx_mini_app_open_id(&self, app_id: &str, code: &str) -> Result<String, AppError> {
        // 按 Module=WeiXinMiniApp, Key=AppId, Value=app_id 查分组
        let app_id_setting = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq("WeiXinMiniApp"))
            .filter(app_setting::Column::Key.eq("AppId"))
            .filter(app_setting::Column::Value.eq(app_id))
            .not_deleted(app_setting::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("微信配置查询失败: {error}")))?
            .ok_or_else(|| AppError::bad_request("未找到对应的微信小程序配置"))?;

        let group_id = app_id_setting
            .group_id
            .ok_or_else(|| AppError::internal("微信小程序配置缺少分组信息"))?;

        // 通过同一 group_id 反查该小程序对应的 AppSecret
        let secret_setting = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq("WeiXinMiniApp"))
            .filter(app_setting::Column::GroupId.eq(group_id))
            .filter(app_setting::Column::Key.eq("AppSecret"))
            .not_deleted(app_setting::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("微信 AppSecret 查询失败: {error}")))?
            .ok_or_else(|| AppError::internal("未找到微信小程序 AppSecret 配置"))?;

        let app_secret = secret_setting.value;

        let url = format!(
            "https://api.weixin.qq.com/sns/jscode2session?appid={app_id}&secret={app_secret}&js_code={code}&grant_type=authorization_code"
        );

        let response = self
            .context
            .http_client_factory
            .client()
            .get(&url)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("微信接口请求失败: {error}")))?;

        let body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("微信接口响应读取失败: {error}")))?;

        // 解析 openid 字段
        let open_id = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("openid")?.as_str().map(str::to_string));

        open_id.ok_or_else(|| AppError::internal(format!("微信 jscode2session 返回无效: {body}")))
    }

    /// 通过 AppId + Code 获取微信公众号 AccessToken 与 OpenId
    #[retry(max_attempts = 3, delay_ms = 200)]
    async fn get_wx_app_access_token_and_open_id(
        &self,
        app_id: &str,
        code: &str,
    ) -> Result<(String, String), AppError> {
        // 公众号配置与小程序配置保持一致 通过 AppId 所在分组反查 AppSecret
        let app_id_setting = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq("WeiXinApp"))
            .filter(app_setting::Column::Key.eq("AppId"))
            .filter(app_setting::Column::Value.eq(app_id))
            .not_deleted(app_setting::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("微信公众号配置查询失败: {error}")))?
            .ok_or_else(|| AppError::bad_request("未找到对应的微信公众号配置"))?;

        let group_id = app_id_setting
            .group_id
            .ok_or_else(|| AppError::internal("微信公众号配置缺少分组信息"))?;

        let secret_setting = app_setting::Entity::find()
            .filter(app_setting::Column::Module.eq("WeiXinApp"))
            .filter(app_setting::Column::GroupId.eq(group_id))
            .filter(app_setting::Column::Key.eq("AppSecret"))
            .not_deleted(app_setting::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("微信公众号 AppSecret 查询失败: {error}")))?
            .ok_or_else(|| AppError::internal("未找到微信公众号 AppSecret 配置"))?;

        let url = format!(
            "https://api.weixin.qq.com/sns/oauth2/access_token?appid={app_id}&secret={}&code={code}&grant_type=authorization_code",
            secret_setting.value,
        );

        let response = self
            .context
            .http_client_factory
            .client()
            .get(&url)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("微信公众号接口请求失败: {error}")))?;

        let body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("微信公众号接口响应读取失败: {error}")))?;

        let payload = serde_json::from_str::<serde_json::Value>(&body)
            .map_err(|error| AppError::internal(format!("微信公众号响应解析失败: {error}")))?;

        let access_token = payload
            .get("access_token")
            .and_then(|item| item.as_str())
            .filter(|item| !item.is_empty())
            .ok_or_else(|| {
                AppError::internal(format!("微信公众号 access_token 返回无效: {body}"))
            })?;
        let open_id = payload
            .get("openid")
            .and_then(|item| item.as_str())
            .filter(|item| !item.is_empty())
            .ok_or_else(|| AppError::internal(format!("微信公众号 openid 返回无效: {body}")))?;

        Ok((access_token.to_string(), open_id.to_string()))
    }

    /// 获取微信公众号用户信息
    async fn get_wx_app_user_info(
        &self,
        access_token: &str,
        open_id: &str,
    ) -> Result<GetWeiXinAppUserInfoDto, AppError> {
        // 公众号网页登录比小程序多一步 userinfo 拉取 用于昵称和头像等资料补全
        let url = format!(
            "https://api.weixin.qq.com/sns/userinfo?access_token={access_token}&openid={open_id}",
        );

        let response = self
            .context
            .http_client_factory
            .client()
            .get(&url)
            .send()
            .await
            .map_err(|error| AppError::internal(format!("微信公众号用户信息请求失败: {error}")))?;

        let body = response
            .text()
            .await
            .map_err(|error| AppError::internal(format!("微信公众号用户信息读取失败: {error}")))?;

        let user_info = serde_json::from_str::<GetWeiXinAppUserInfoDto>(&body)
            .map_err(|error| AppError::internal(format!("微信公众号用户信息解析失败: {error}")))?;

        if user_info.open_id.as_deref().is_none_or(str::is_empty) {
            return Err(AppError::internal(format!(
                "微信公众号用户信息返回无效: {body}"
            )));
        }

        Ok(user_info)
    }

    /// 发送短信验证码
    pub async fn send_sms_verify_code(
        &self,
        request: SendSmsVerifyCodeRequest,
    ) -> Result<bool, AppError> {
        Self::validate_send_sms_verify_code_request(&request)?;

        let phone = request.phone.trim();
        let code_key = build_phone_verify_code_key(phone);

        // 与 .NET 行为对齐：验证码键存在期间不允许重复获取
        if let Some(cache) = self.context.cache {
            if cache.get_string(&code_key).await?.is_some() {
                return Err(AppError::bad_request("请勿频繁获取验证码！"));
            }
        }

        // 验证码只入缓存 真正的短信发送交给任务中心异步处理
        let code = rand::random_range(100000..1_000_000u32).to_string();
        let mut template_params = IndexMap::new();
        template_params.insert("code".to_string(), code.clone());

        let task_center_service = TaskCenterService::new(self.context.app);
        let payload = serde_json::to_string(&SendSmsMessage {
            phone: phone.to_string(),
            sign_name: self.context.sms_sign_name.to_string(),
            template_code: self.context.sms_template_code.to_string(),
            template_params,
        })
        .map_err(|error| AppError::internal(format!("短信任务参数序列化失败: {error}")))?;

        task_center_service
            .create_queue_task_single(CreateQueueTaskDto {
                name: self.context.sms_queue_task_name.to_string(),
                parameter: Some(payload),
                plan_time: None,
                callback_name: None,
                callback_parameter: None,
                parent_task_id: None,
            })
            .await?;

        if let Some(cache) = self.context.cache {
            // 与 .NET 保持一致：只写验证码键，键存在期间即视为发送冷却中
            cache
                .set_string_with_options(
                    &code_key,
                    &code,
                    CacheEntryOptions::relative(std::time::Duration::from_secs(
                        self.context.sms_verify_code_ttl_seconds,
                    )),
                )
                .await?;
        }

        Ok(true)
    }

    /// 获取图形验证码图片
    pub async fn get_verify_code(&self, sign: Uuid) -> Result<Vec<u8>, AppError> {
        let Some(cache) = self.context.cache else {
            return Err(AppError::internal("当前宿主未启用缓存，无法生成图形验证码"));
        };

        // 图形验证码与 NetEngine 保持一致：四位数字且首位非零
        let verify_code = rand::random_range(1000..9999u32).to_string();

        // 先生成图片再写缓存 避免图片生成失败时留下无效验证码
        let image_bytes = img_helper::get_verify_code(&verify_code)
            .map_err(|error| AppError::internal(format!("生成图形验证码失败: {error}")))?;

        cache
            .set_string_with_options(
                &build_graphic_verify_code_key(sign),
                &verify_code,
                CacheEntryOptions::relative(std::time::Duration::from_secs(300)),
            )
            .await?;

        Ok(image_bytes)
    }

    /// 通过短信验证码修改账户密码
    pub async fn update_password_by_sms(
        &self,
        current_user: CurrentUser,
        request: UpdatePasswordBySmsRequest,
    ) -> Result<bool, AppError> {
        Self::validate_update_password_by_sms_request(&request)?;

        let user_model = user::Entity::find_by_id(current_user.user_id)
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户信息查询失败: {error}")))?;

        let Some(user_model) = user_model else {
            return Err(AppError::bad_request("账户不存在"));
        };

        let before_log = json!({
            "id": user_model.id,
            "password_updated": false,
            "update_mode": "sms",
        });

        // 使用当前账号绑定手机号完成验证码校验
        self.verify_phone_code(&user_model.phone, request.sms_code.trim())
            .await?;

        let mut active = user_model.into_active_model();
        active.password = Set(hash_password_pbkdf2_sha256(
            &request.new_password,
            &current_user.user_id.to_string(),
        ));
        apply_update_user_audit_field(&mut active.update_user_id, &current_user);
        active
            .update(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("密码更新失败: {error}")))?;

        self.invalidate_user_tokens(current_user.user_id).await?;

        // 记录短信验证码改密写操作
        write_data_update_log(
            self.context.app,
            "User",
            current_user.user_id,
            "update_password_by_sms",
            Some(before_log),
            Some(json!({
                "id": current_user.user_id,
                "password_updated": true,
                "update_mode": "sms",
            })),
            Some(&current_user),
        )
        .await?;

        Ok(true)
    }

    /// 通过旧密码修改账户密码
    pub async fn update_password_by_old_password(
        &self,
        current_user: CurrentUser,
        request: UpdatePasswordByOldPasswordRequest,
    ) -> Result<bool, AppError> {
        Self::validate_update_password_by_old_password_request(&request)?;

        let user_model = user::Entity::find_by_id(current_user.user_id)
            .not_deleted(user::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户信息查询失败: {error}")))?;

        let Some(user_model) = user_model else {
            return Err(AppError::bad_request("账户不存在"));
        };

        let before_log = json!({
            "id": user_model.id,
            "password_updated": false,
            "update_mode": "old_password",
        });

        if !verify_password_pbkdf2_sha256(
            &request.old_password,
            &current_user.user_id.to_string(),
            &user_model.password,
        ) {
            return Err(AppError::bad_request("原始密码验证失败"));
        }

        let mut active = user_model.into_active_model();
        active.password = Set(hash_password_pbkdf2_sha256(
            &request.new_password,
            &current_user.user_id.to_string(),
        ));
        apply_update_user_audit_field(&mut active.update_user_id, &current_user);
        active
            .update(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("密码更新失败: {error}")))?;

        self.invalidate_user_tokens(current_user.user_id).await?;

        // 记录旧密码改密写操作
        write_data_update_log(
            self.context.app,
            "User",
            current_user.user_id,
            "update_password_by_old_password",
            Some(before_log),
            Some(json!({
                "id": current_user.user_id,
                "password_updated": true,
                "update_mode": "old_password",
            })),
            Some(&current_user),
        )
        .await?;

        Ok(true)
    }

    /// 创建用户 Token 记录
    async fn create_user_token_record(
        &self,
        user_id: i64,
        last_token_id: Option<i64>,
    ) -> Result<i64, AppError> {
        let token_id = self
            .context
            .app
            .id_generator
            .get_id()
            .await
            .map_err(|error| AppError::internal(format!("生成用户 Token 主键失败: {error}")))?;
        let audit_fields = build_system_create_audit_fields();

        let user_token_active_model = user_token::ActiveModel {
            id: Set(token_id),
            user_id: Set(user_id),
            last_id: Set(last_token_id),
            create_time: Set(audit_fields.create_time),
            delete_time: Set(audit_fields.delete_time),
        };

        user_token_active_model
            .insert(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("Token 记录保存失败: {error}")))?;

        try_cache_token_validity(self.context.cache, user_id, token_id, true).await;

        Ok(token_id)
    }

    /// 生成 JWT Token 字符串
    fn build_jwt_token(&self, user_id: i64, token_id: i64) -> Result<String, AppError> {
        let now = Utc::now();
        let claims = JwtClaims {
            token_id: token_id.to_string(),
            user_id: user_id.to_string(),
            iss: self.context.issuer.to_string(),
            aud: self.context.audience.to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::seconds(self.context.expiry_seconds)).timestamp(),
            nbf: now.timestamp(),
        };

        self.context.jwt_codec.sign(&claims)
    }

    /// 根据用户 ID 生成 JWT Token
    pub async fn get_token_by_user_id(
        &self,
        user_id: i64,
        last_token_id: Option<i64>,
    ) -> Result<String, AppError> {
        // 先落库 Token 链记录 再返回签名后的 JWT 字符串
        let token_id = self
            .create_user_token_record(user_id, last_token_id)
            .await?;
        self.build_jwt_token(user_id, token_id)
    }

    /// 尝试为当前 Token 续签
    /// 剩余有效期不足总有效期一半时签发新 Token，并写入缓存防止并发重复签发
    /// 同时机会性清理 7 天前的过期 Token 记录，通过缓存标记控制清理频率为每小时最多一次
    pub async fn try_renew_token(
        &self,
        user_id: i64,
        token_id: i64,
        nbf: i64,
        exp: i64,
    ) -> Result<Option<String>, AppError> {
        // 计算有效期中间时间点，未过半则不续签
        let now = Utc::now().timestamp();
        let half_life = nbf + (exp - nbf) / 2;

        if now <= half_life {
            return Ok(None);
        }

        // 已有续签缓存时直接返回，避免重复签发
        let renew_cache_key = format!("token:renew:{token_id}");

        if let Some(cache) = self.context.cache {
            if let Some(cached_token) = cache.get_string(&renew_cache_key).await? {
                return Ok(Some(cached_token));
            }
        }

        let renew_lock_key = format!("token:renew:lock:{token_id}");
        let renew_lock_guard = self
            .context
            .authorize_lock
            .try_lock(&renew_lock_key, std::time::Duration::from_secs(10), 1)
            .await?;

        if renew_lock_guard.is_none() {
            if let Some(cache) = self.context.cache {
                if let Some(cached_token) = cache.get_string(&renew_cache_key).await? {
                    return Ok(Some(cached_token));
                }
            }

            return Ok(None);
        }

        if let Some(cache) = self.context.cache {
            if let Some(cached_token) = cache.get_string(&renew_cache_key).await? {
                return Ok(Some(cached_token));
            }
        }

        // 签发新 Token，保留前驱 Token ID 用于追溯 Token 链
        let new_token_id = self
            .create_user_token_record(user_id, Some(token_id))
            .await?;
        let new_token = self.build_jwt_token(user_id, new_token_id)?;

        // 记录续签生成的新 Token 写入
        write_data_update_log(
            self.context.app,
            "UserToken",
            new_token_id,
            "renew_token",
            None,
            Some(json!({
                "id": new_token_id,
                "user_id": user_id,
                "last_id": token_id,
            })),
            None,
        )
        .await?;

        if let Some(cache) = self.context.cache {
            // 缓存新 Token 10 分钟，防止并发请求触发重复签发
            let _ = cache
                .set_string_with_options(
                    &renew_cache_key,
                    &new_token,
                    CacheEntryOptions::relative(std::time::Duration::from_secs(600)),
                )
                .await;
        }

        let cleanup_lock = self
            .context
            .authorize_lock
            .try_lock(
                "token:cleanup:lock",
                std::time::Duration::from_secs(3600),
                1,
            )
            .await?;
        if cleanup_lock.is_some() {
            let clear_time: DateTimeWithTimeZone = (Utc::now() - Duration::days(7)).into();
            let _ = user_token::Entity::delete_many()
                .filter(user_token::Column::CreateTime.lt(clear_time))
                .exec(self.context.app.db)
                .await;
        }

        Ok(Some(new_token))
    }

    /// 检测当前用户是否具备指定路由权限
    pub async fn check_function_authorize(
        &self,
        current_user: CurrentUser,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<bool, AppError> {
        // 缓存键中包含路由版本和用户版本 便于低成本整体失效。
        // 路由绑定和功能有效性仍需在缓存命中前确认，避免功能被软删除后旧的 true 缓存继续放行。
        let cache_key = self
            .build_route_authorize_cache_key(current_user.user_id, module, route, allow_unbound)
            .await?;

        let function_id = function_route::Entity::find()
            .filter(function_route::Column::Module.eq(module))
            .filter(function_route::Column::Route.eq(route))
            .not_deleted(function_route::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("路由权限信息查询失败: {error}")))?
            .and_then(|item| item.function_id);

        let result = if let Some(function_id) = function_id {
            let function_is_active = function::Entity::find_by_id(function_id)
                .not_deleted(function::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("功能信息查询失败: {error}")))?
                .is_some();

            if !function_is_active {
                self.try_cache_bool(&cache_key, false).await;
                return Ok(false);
            }

            if let Some(cache) = self.context.cache {
                if let Some(cached_value) = cache.get_string(&cache_key).await? {
                    return parse_cached_bool(&cached_value);
                }
            }

            // 已绑定功能时 同时检查用户直授和角色继承的权限
            let role_ids = user_role::Entity::find()
                .filter(user_role::Column::UserId.eq(current_user.user_id))
                .not_deleted(user_role::Column::DeleteTime)
                .all(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("用户角色查询失败: {error}")))?
                .into_iter()
                .map(|item| item.role_id)
                .collect::<Vec<_>>();

            let mut authorize_condition =
                Condition::any().add(function_authorize::Column::UserId.eq(current_user.user_id));

            if !role_ids.is_empty() {
                authorize_condition =
                    authorize_condition.add(function_authorize::Column::RoleId.is_in(role_ids));
            }

            function_authorize::Entity::find()
                .filter(function_authorize::Column::FunctionId.eq(function_id))
                .not_deleted(function_authorize::Column::DeleteTime)
                .filter(authorize_condition)
                .one(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("权限数据查询失败: {error}")))?
                .is_some()
        } else {
            if let Some(cache) = self.context.cache {
                if let Some(cached_value) = cache.get_string(&cache_key).await? {
                    return parse_cached_bool(&cached_value);
                }
            }

            allow_unbound
        };

        self.try_cache_bool(&cache_key, result).await;

        Ok(result)
    }

    /// 获取当前用户可访问的功能列表
    pub async fn get_function_list(
        &self,
        current_user: CurrentUser,
        sign: Option<&str>,
    ) -> Result<HashMap<String, String>, AppError> {
        // 功能列表与用户和筛选条件强相关 因此单独构建缓存键
        let cache_key = self
            .build_function_list_cache_key(current_user.user_id, sign)
            .await?;

        if let Some(cache) = self.context.cache {
            if let Some(cached_value) = cache.get_string(&cache_key).await? {
                let cached_result = serde_json::from_str::<HashMap<String, String>>(&cached_value)
                    .map_err(|error| {
                        AppError::internal(format!("反序列化功能列表缓存失败: {error}"))
                    })?;
                return Ok(cached_result);
            }
        }

        let role_ids = user_role::Entity::find()
            .filter(user_role::Column::UserId.eq(current_user.user_id))
            .not_deleted(user_role::Column::DeleteTime)
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户角色查询失败: {error}")))?
            .into_iter()
            .map(|item| item.role_id)
            .collect::<Vec<_>>();

        let mut authorize_condition =
            Condition::any().add(function_authorize::Column::UserId.eq(current_user.user_id));

        if !role_ids.is_empty() {
            authorize_condition =
                authorize_condition.add(function_authorize::Column::RoleId.is_in(role_ids));
        }

        let mut query = function_authorize::Entity::find()
            .find_also_related(repository::function::Entity)
            .not_deleted(function_authorize::Column::DeleteTime)
            .filter(repository::function::Column::DeleteTime.is_null())
            .filter(authorize_condition);

        if let Some(sign) = sign.map(str::trim).filter(|item| !item.is_empty()) {
            query = query.filter(repository::function::Column::Sign.eq(sign));
        }

        // 同一功能可能通过多条授权命中 这里用 Map 去重
        let mut result = HashMap::new();

        for (_, function) in query
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("功能权限列表查询失败: {error}")))?
        {
            let Some(function) = function else {
                continue;
            };

            result.entry(function.sign).or_insert(function.name);
        }

        self.try_cache_function_list(&cache_key, &result).await?;

        Ok(result)
    }

    /// 获取路由权限绑定列表
    pub async fn get_function_route_list(
        &self,
        request: GetFunctionRouteListRequest,
    ) -> Result<Vec<FunctionRouteDto>, AppError> {
        // 先做数据库粗筛 再在内存中补充关键词匹配
        let mut query = function_route::Entity::find()
            .find_also_related(function::Entity)
            .not_deleted(function_route::Column::DeleteTime);

        if let Some(module) = request
            .module
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            query = query.filter(function_route::Column::Module.eq(module));
        }

        let keyword = request
            .keyword
            .as_deref()
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned);

        let route_list = query
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("路由权限绑定列表查询失败: {error}")))?;

        Ok(route_list
            .into_iter()
            .filter(|(route, function)| match keyword.as_deref() {
                Some(keyword) => {
                    route.route.contains(keyword)
                        || route
                            .remark
                            .as_deref()
                            .is_some_and(|item| item.contains(keyword))
                        || function
                            .as_ref()
                            .is_some_and(|item| item.name.contains(keyword))
                        || function
                            .as_ref()
                            .is_some_and(|item| item.sign.contains(keyword))
                }
                None => true,
            })
            .map(|(route, function)| FunctionRouteDto {
                id: route.id.to_string(),
                module: route.module,
                route: route.route,
                remark: route.remark,
                function_id: function.as_ref().map(|item| item.id.to_string()),
                function_name: function.as_ref().map(|item| item.name.clone()),
                function_sign: function.as_ref().map(|item| item.sign.clone()),
                create_time: route.create_time.to_rfc3339(),
            })
            .collect())
    }

    /// 设置路由权限绑定
    pub async fn set_function_route(&self, request: SetFunctionRouteDto) -> Result<bool, AppError> {
        // 仅允许把路由绑定到叶子功能节点 避免绑定模块节点导致语义混乱
        if request.route_id <= 0 {
            return Err(AppError::bad_request("无效的 routeId"));
        }

        if let Some(function_id) = request.function_id {
            if function_id <= 0 {
                return Err(AppError::bad_request("无效的 functionId"));
            }

            let function_model = function::Entity::find_by_id(function_id)
                .not_deleted(function::Column::DeleteTime)
                .one(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("功能信息查询失败: {error}")))?;

            let Some(function_model) = function_model else {
                return Err(AppError::bad_request("指定的功能不存在"));
            };

            if function_model.r#type != FUNCTION_TYPE_FUNCTION {
                return Err(AppError::bad_request("路由仅允许绑定功能节点"));
            }
        }

        let route_model = function_route::Entity::find_by_id(request.route_id)
            .not_deleted(function_route::Column::DeleteTime)
            .one(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("路由权限信息查询失败: {error}")))?;

        let Some(route_model) = route_model else {
            return Err(AppError::bad_request("指定的路由不存在"));
        };

        let before_log = json!({
            "id": route_model.id,
            "module": route_model.module,
            "route": route_model.route,
            "remark": route_model.remark,
            "function_id": route_model.function_id,
        });

        let mut active = route_model.into_active_model();
        active.function_id = Set(request.function_id);
        let updated_model = active
            .update(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("路由权限绑定更新失败: {error}")))?;

        invalidate_route_authorize_cache(self.context.cache).await;

        // 记录路由权限绑定关系变更
        write_data_update_log(
            self.context.app,
            "FunctionRoute",
            updated_model.id,
            "set_function_route",
            Some(before_log),
            Some(json!({
                "id": updated_model.id,
                "module": updated_model.module,
                "route": updated_model.route,
                "remark": updated_model.remark,
                "function_id": updated_model.function_id,
            })),
            None,
        )
        .await?;

        Ok(true)
    }

    /// 获取功能树
    pub async fn get_function_tree(&self) -> Result<Vec<FunctionTreeDto>, AppError> {
        load_function_tree(self.context.app.db, None, None)
            .await
            .map_err(|error| AppError::internal(error.to_string()))
    }

    /// 构建路由权限缓存键
    async fn build_route_authorize_cache_key(
        &self,
        user_id: i64,
        module: &str,
        route: &str,
        allow_unbound: bool,
    ) -> Result<String, AppError> {
        let route_version = get_route_authorize_version(self.context.cache).await?;
        let user_version = get_user_authorize_version(self.context.cache, user_id).await?;

        Ok(format!(
            "authorize:route:{route_version}:{user_version}:{user_id}:{module}:{route}:{allow_unbound}"
        ))
    }

    /// 构建功能列表缓存键
    async fn build_function_list_cache_key(
        &self,
        user_id: i64,
        sign: Option<&str>,
    ) -> Result<String, AppError> {
        let user_version = get_user_authorize_version(self.context.cache, user_id).await?;
        let normalized_sign = sign
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(Cow::from)
            .unwrap_or_else(|| Cow::from("*"));

        Ok(format!(
            "authorize:function_list:{user_version}:{user_id}:{normalized_sign}"
        ))
    }

    /// 校验手机号验证码
    async fn verify_phone_code(&self, phone: &str, verify_code: &str) -> Result<(), AppError> {
        let Some(cache) = self.context.cache else {
            return Err(AppError::internal("当前宿主未启用缓存，无法校验短信验证码"));
        };

        let code_key = build_phone_verify_code_key(phone);
        let cached_code = cache.get_string(&code_key).await?;

        match cached_code.as_deref() {
            Some(code) if code == verify_code => {
                cache.remove(&code_key).await?;
                Ok(())
            }
            _ => Err(AppError::bad_request("短信验证码错误")),
        }
    }

    /// 使指定用户的历史 Token 失效
    async fn invalidate_user_tokens(&self, user_id: i64) -> Result<(), AppError> {
        let token_list = user_token::Entity::find()
            .filter(user_token::Column::UserId.eq(user_id))
            .not_deleted(user_token::Column::DeleteTime)
            .all(self.context.app.db)
            .await
            .map_err(|error| AppError::internal(format!("用户 Token 查询失败: {error}")))?;

        for token in token_list {
            let token_id = token.id;
            let mut token_active = token.into_active_model();
            token_active.delete_time = Set(Some(Utc::now().into()));
            token_active
                .update(self.context.app.db)
                .await
                .map_err(|error| AppError::internal(format!("用户 Token 失效失败: {error}")))?;

            try_cache_token_validity(self.context.cache, user_id, token_id, false).await;
        }

        Ok(())
    }

    /// 尝试写入布尔缓存
    async fn try_cache_bool(&self, key: &str, value: bool) {
        if let Some(cache) = self.context.cache {
            let _ = cache
                .set_string_with_options(
                    key,
                    if value { "1" } else { "0" },
                    CacheEntryOptions::relative(std::time::Duration::from_secs(300)),
                )
                .await;
        }
    }

    /// 尝试写入功能列表缓存
    async fn try_cache_function_list(
        &self,
        key: &str,
        value: &HashMap<String, String>,
    ) -> Result<(), AppError> {
        if let Some(cache) = self.context.cache {
            let payload = serde_json::to_string(value)
                .map_err(|error| AppError::internal(format!("序列化功能列表缓存失败: {error}")))?;
            let _ = cache
                .set_string_with_options(
                    key,
                    &payload,
                    CacheEntryOptions::relative(std::time::Duration::from_secs(300)),
                )
                .await;
        }

        Ok(())
    }
}

/// 使指定用户的授权缓存失效
pub async fn invalidate_user_authorize_cache(
    cache: Option<&dyn application_interface::cache::DistributedCache>,
    user_id: i64,
) {
    let Some(cache) = cache else {
        return;
    };

    let version = get_user_authorize_version(Some(cache), user_id)
        .await
        .unwrap_or(1)
        + 1;
    let _ = cache
        .set_string_with_options(
            &format!("authorize:user:version:{user_id}"),
            &version.to_string(),
            CacheEntryOptions::new(),
        )
        .await;

    set_user_authorize_version_local(user_id, version);
}

/// 使路由权限缓存失效
pub async fn invalidate_route_authorize_cache(
    cache: Option<&dyn application_interface::cache::DistributedCache>,
) {
    let Some(cache) = cache else {
        return;
    };

    let version = get_route_authorize_version(Some(cache)).await.unwrap_or(1) + 1;
    let _ = cache
        .set_string_with_options(
            AUTHORIZE_ROUTE_VERSION_KEY,
            &version.to_string(),
            CacheEntryOptions::new(),
        )
        .await;

    set_route_authorize_version_local(version);
}

/// 获取路由权限缓存版本
async fn get_route_authorize_version(
    cache: Option<&dyn application_interface::cache::DistributedCache>,
) -> Result<u64, AppError> {
    if let Some(version) = try_get_route_authorize_version_local() {
        return Ok(version);
    }

    let Some(cache) = cache else {
        return Ok(1);
    };

    let version = cache
        .get_string(AUTHORIZE_ROUTE_VERSION_KEY)
        .await?
        .as_deref()
        .unwrap_or("1")
        .parse::<u64>()
        .map_err(|error| AppError::internal(format!("解析路由权限缓存版本失败: {error}")))?;

    set_route_authorize_version_local(version);

    Ok(version)
}

/// 获取用户授权缓存版本
async fn get_user_authorize_version(
    cache: Option<&dyn application_interface::cache::DistributedCache>,
    user_id: i64,
) -> Result<u64, AppError> {
    if let Some(version) = try_get_user_authorize_version_local(user_id) {
        return Ok(version);
    }

    let Some(cache) = cache else {
        return Ok(1);
    };

    let version = cache
        .get_string(&format!("authorize:user:version:{user_id}"))
        .await?
        .as_deref()
        .unwrap_or("1")
        .parse::<u64>()
        .map_err(|error| AppError::internal(format!("解析用户授权缓存版本失败: {error}")))?;

    set_user_authorize_version_local(user_id, version);

    Ok(version)
}

/// 解析布尔缓存值
fn parse_cached_bool(value: &str) -> Result<bool, AppError> {
    match value {
        "1" => Ok(true),
        "0" => Ok(false),
        _ => Err(AppError::internal("布尔缓存值格式无效")),
    }
}

/// 构建 Token 有效性缓存键
pub fn build_token_valid_cache_key(user_id: i64, token_id: i64) -> String {
    format!("authorize:token_valid:{user_id}:{token_id}")
}

/// 尝试写入 Token 有效性缓存
pub async fn try_cache_token_validity(
    cache: Option<&dyn application_interface::cache::DistributedCache>,
    user_id: i64,
    token_id: i64,
    is_valid: bool,
) {
    let Some(cache) = cache else {
        return;
    };

    let _ = cache
        .set_string_with_options(
            &build_token_valid_cache_key(user_id, token_id),
            if is_valid { "1" } else { "0" },
            CacheEntryOptions::relative(std::time::Duration::from_secs(
                TOKEN_VALID_CACHE_TTL_SECONDS,
            )),
        )
        .await;
}

/// 构建手机号验证码缓存键
fn build_phone_verify_code_key(phone: &str) -> String {
    format!("VerifyPhone_{}", phone.trim())
}

/// 构建图形验证码缓存键
fn build_graphic_verify_code_key(sign: Uuid) -> String {
    format!("VerifyCode{sign}")
}

/// 递归加载功能树
#[async_recursion]
async fn load_function_tree(
    db: &DatabaseConnection,
    parent_id: Option<i64>,
    parent_name: Option<String>,
) -> Result<Vec<FunctionTreeDto>, DbErr> {
    // 先取当前层模块 再批量加载其直接功能节点和递归子模块
    let module_query = match parent_id {
        Some(parent_id) => {
            function::Entity::find().filter(function::Column::ParentId.eq(parent_id))
        }
        None => function::Entity::find().filter(function::Column::ParentId.is_null()),
    };

    let module_list = module_query
        .filter(function::Column::Type.eq(FUNCTION_TYPE_MODULE))
        .not_deleted(function::Column::DeleteTime)
        .order_by_asc(function::Column::Id)
        .all(db)
        .await?;

    if module_list.is_empty() {
        return Ok(vec![]);
    }

    let module_ids = module_list.iter().map(|item| item.id).collect::<Vec<_>>();

    let child_functions = function::Entity::find()
        .filter(function::Column::ParentId.is_in(module_ids.clone()))
        .filter(function::Column::Type.eq(FUNCTION_TYPE_FUNCTION))
        .not_deleted(function::Column::DeleteTime)
        .order_by_asc(function::Column::Id)
        .all(db)
        .await?;

    let mut result = Vec::with_capacity(module_list.len());

    for module in module_list {
        // 子功能名称会裁掉父模块前缀 让前端展示更简洁
        let children = load_function_tree(db, Some(module.id), Some(module.name.clone())).await?;
        let functions = child_functions
            .iter()
            .filter(|item| item.parent_id == Some(module.id))
            .map(|item| FunctionTreeDto {
                id: item.id.to_string(),
                name: trim_prefix_name(&item.name, Some(&module.name)),
                sign: item.sign.clone(),
                remark: item.remark.clone(),
                child_list: vec![],
                function_list: vec![],
            })
            .collect::<Vec<_>>();

        result.push(FunctionTreeDto {
            id: module.id.to_string(),
            name: trim_prefix_name(&module.name, parent_name.as_deref()),
            sign: module.sign,
            remark: module.remark,
            child_list: children,
            function_list: functions,
        });
    }

    Ok(result)
}
